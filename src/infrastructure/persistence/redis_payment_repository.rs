use async_trait::async_trait;
use redis::{AsyncCommands, Client, Script};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use crate::domain::payment::Payment;
use crate::domain::repository::PaymentRepository;
use crate::infrastructure::config::redis::PROCESSED_PAYMENTS_SET_KEY;

#[derive(Clone)]
pub struct RedisPaymentRepository {
	client: Client,
}

impl RedisPaymentRepository {
	pub fn new(client: Client) -> Self {
		Self { client }
	}

	async fn calculate_payments_summary_using_lua(
		con: &mut redis::aio::MultiplexedConnection,
		group: &str,
		from_ts: i128,
		to_ts: i128,
	) -> redis::RedisResult<(usize, f64)> {
		let lua = Script::new(
			r#"
            local ids = redis.call("ZRANGEBYSCORE", KEYS[1], ARGV[1], ARGV[2])
            local total_requests = 0
            local total_amount = 0.0

            for i, id in ipairs(ids) do
                local key = ARGV[3] .. ":" .. id
                local amount = redis.call("HGET", key, "amount")
                if amount then
                    total_requests = total_requests + 1
                    total_amount = total_amount + tonumber(amount)
                end
            end

            return {tostring(total_requests), tostring(total_amount)}
        "#,
		);

		let response: (String, String) = lua
			.key(PROCESSED_PAYMENTS_SET_KEY)
			.arg(from_ts)
			.arg(to_ts)
			.arg(format!("payment_summary:{group}"))
			.invoke_async(con)
			.await?;

		Ok((
			response.0.parse().unwrap_or_default(),
			response.1.parse().unwrap_or_default(),
		))
	}
}

#[async_trait]
impl PaymentRepository for RedisPaymentRepository {
	async fn save(
		&self,
		payment: Payment,
	) -> Result<(), Box<dyn std::error::Error + Send>> {
		let mut con = self
			.client
			.get_multiplexed_async_connection()
			.await
			.map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send>)?;

		let payment_id = payment.correlation_id.to_string();
		let payment_group = payment.processed_by.unwrap_or_default();
		let payment_key = format!("payment_summary:{payment_group}:{payment_id}");

		redis::pipe()
			.atomic()
			.hset(&payment_key, "amount", format!("{:.2}", payment.amount))
			.hset_multiple(&payment_key, &[
				(
					"requested_at",
					payment
						.requested_at
						.map(|ts| ts.to_string())
						.unwrap_or_default(),
				),
				(
					"processed_at",
					payment
						.processed_at
						.map(|ts| ts.to_string())
						.unwrap_or_default(),
				),
				("processed_by", payment_group),
			])
			.ignore()
			.zadd(
				PROCESSED_PAYMENTS_SET_KEY,
				payment_id,
				payment
					.requested_at
					.map(|ts| ts.unix_timestamp_nanos())
					.unwrap_or_default(),
			)
			.query_async::<()>(&mut con)
			.await
			.map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send>)?;

		Ok(())
	}

	async fn get_summary_by_group(
		&self,
		group: &str,
		from_ts: OffsetDateTime,
		to_ts: OffsetDateTime,
	) -> Result<(usize, f64), Box<dyn std::error::Error + Send>> {
		let mut con = self
			.client
			.clone()
			.get_multiplexed_async_connection()
			.await
			.map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send>)?;
		let (req, amt) = Self::calculate_payments_summary_using_lua(
			&mut con,
			group,
			from_ts.unix_timestamp_nanos(),
			to_ts.unix_timestamp_nanos(),
		)
		.await
		.map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send>)?;
		Ok((req, amt))
	}

	async fn get_payment_summary(
		&self,
		group: &str,
		payment_id: &str,
	) -> Result<Payment, Box<dyn std::error::Error + Send>> {
		let mut con = self
			.client
			.get_multiplexed_async_connection()
			.await
			.map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send>)?;

		let payment_key = format!("payment_summary:{group}:{payment_id}");
		log::debug!("Retrieving payment summary for key: {}", payment_key);
		let payment_data: Option<std::collections::HashMap<String, String>> =
			con.hgetall(&payment_key).await.ok();

		if let Some(map) = payment_data &&
			let Some(amount_str) = map.get("amount") &&
			let Ok(amount) = amount_str.parse::<f64>()
		{
			let requested_at = map
				.get("requested_at")
				.and_then(|odt| OffsetDateTime::parse(odt, &Rfc3339).ok());
			let processed_at = map
				.get("processed_at")
				.and_then(|odt| OffsetDateTime::parse(odt, &Rfc3339).ok());
			let processed_by = map.get("processed_by").cloned();

			let payment = Payment {
				correlation_id: uuid::Uuid::parse_str(payment_id)
					.expect("Valid UUID"),
				amount,
				requested_at,
				processed_at,
				processed_by,
			};
			return Ok(payment);
		}

		Err(Box::new(std::io::Error::new(
			std::io::ErrorKind::NotFound,
			"Payment not found",
		)))
	}

	async fn is_already_processed(
		&self,
		payment_id: &str,
	) -> Result<bool, Box<dyn std::error::Error + Send>> {
		let mut con = self
			.client
			.clone()
			.get_multiplexed_async_connection()
			.await
			.map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send>)?;

		let is_already_processed: Option<f64> = con
			.zscore(PROCESSED_PAYMENTS_SET_KEY, payment_id)
			.await
			.ok();

		Ok(is_already_processed.is_some())
	}

	async fn clear(&self) -> Result<(), Box<dyn std::error::Error + Send>> {
		let mut con = self
			.client
			.get_multiplexed_async_connection()
			.await
			.map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send>)?;

		let keys: Vec<String> = con
			.keys("payment_summary:*")
			.await
			.map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send>)?;

		let _: () = con
			.del(keys)
			.await
			.map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send>)?;

		let _: () = con
			.del(PROCESSED_PAYMENTS_SET_KEY)
			.await
			.map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send>)?;

		Ok(())
	}
}
