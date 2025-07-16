use std::time::Duration;

use chrono::Utc;
use log::{error, info, warn};
use redis::{AsyncCommands, RedisResult};
use reqwest::Client;
use tokio::time::sleep;

use crate::config::{
	DEFAULT_PAYMENT_SUMMARY_KEY, DEFAULT_PROCESSOR_HEALTH_KEY,
	FALLBACK_PAYMENT_SUMMARY_KEY, FALLBACK_PROCESSOR_HEALTH_KEY, PAYMENTS_QUEUE_KEY,
	PROCESSED_PAYMENTS_SET_KEY,
};
use crate::model::internal::Payment;
use crate::model::payment_processor::PaymentProcessorRequest;

pub async fn payment_processing_worker(
	redis_client: redis::Client,
	client: Client,
	default_url: String,
	fallback_url: String,
) {
	loop {
		let mut con = match redis_client.get_multiplexed_async_connection().await {
			Ok(con) => con,
			Err(e) => {
				error!(
					"Payment processing worker failed to get Redis connection: {e}"
				);
				sleep(Duration::from_secs(1)).await;
				continue;
			}
		};

		let popped_value: Option<(String, String)> =
			match con.brpop(PAYMENTS_QUEUE_KEY, 0.0).await {
				Ok(val) => val,
				Err(e) => {
					error!("Failed to pop from payments queue: {e}");
					sleep(Duration::from_secs(1)).await;
					continue;
				}
			};

		let payment_str = if let Some((_key, val)) = popped_value {
			val
		} else {
			info!("No payments in queue, waiting...");
			sleep(Duration::from_secs(1)).await;
			continue;
		};

		let payment: Payment = match serde_json::from_str(&payment_str) {
			Ok(p) => p,
			Err(e) => {
				warn!(
					"Failed to deserialize payment request from queue: {e}. \
					 Original string: {payment_str}"
				);
				continue;
			}
		};

		if is_already_processed(&mut con, &payment).await {
			info!(
				"Skipping already processed payment: {}",
				payment.correlation_id
			);
			continue;
		}

		let default_failing =
			is_backend_failing_or_slow(DEFAULT_PROCESSOR_HEALTH_KEY, &mut con).await;
		let mut processed = false;

		if !default_failing {
			processed = process_payment(
				&default_url,
				DEFAULT_PAYMENT_SUMMARY_KEY,
				&payment,
				&client,
				&mut con,
			)
			.await;
		}

		let fallback_failing: bool =
			is_backend_failing_or_slow(FALLBACK_PROCESSOR_HEALTH_KEY, &mut con)
				.await;

		if !processed && !fallback_failing {
			processed = process_payment(
				&fallback_url,
				FALLBACK_PAYMENT_SUMMARY_KEY,
				&payment,
				&client,
				&mut con,
			)
			.await;
		}

		if !processed {
			warn!(
				"Payment {} could not be processed by any processor. Re-queueing.",
				payment.correlation_id
			);
			let _: Result<(), _> = con
				.lpush(PAYMENTS_QUEUE_KEY, serde_json::to_string(&payment).unwrap())
				.await;
		}
	}
}

async fn process_payment(
	processor_url: &String,
	processor_summary_key: &str,
	payment: &Payment,
	client: &Client,
	con: &mut redis::aio::MultiplexedConnection,
) -> bool {
	let payment_request = PaymentProcessorRequest {
		correlation_id: payment.correlation_id,
		amount:         payment.amount,
		requested_at:   Utc::now(),
	};
	match client
		.post(format!("{processor_url}/payments"))
		.json(&payment_request)
		.send()
		.await
	{
		Ok(resp) => {
			if resp.status().is_success() {
				let _ = record_payment_processed(
					&payment_request,
					processor_summary_key,
					con,
				)
				.await;
				true
			} else {
				error!(
					"Processor returned non-success status for {}: {}",
					payment.correlation_id,
					resp.status()
				);
				false
			}
		}
		Err(e) => {
			error!(
				"Failed to send payment {} to processor: {e}",
				payment.correlation_id
			);
			false
		}
	}
}

async fn record_payment_processed(
	payment_request: &PaymentProcessorRequest,
	payment_summary_key: &str,
	con: &mut redis::aio::MultiplexedConnection,
) -> RedisResult<i64> {
	let payment_processed_key =
		payment_processed_key_of(payment_summary_key, payment_request);
	return redis::pipe()
		.hset(&payment_processed_key, "amount", payment_request.amount)
		.hset(
			&payment_processed_key,
			"processed_at",
			payment_request.requested_at.timestamp(),
		)
		.ignore()
		.cmd("ZADD")
		.arg(PROCESSED_PAYMENTS_SET_KEY)
		.arg(payment_request.requested_at.timestamp())
		.arg(payment_request.correlation_id.to_string())
		.query_async::<i64>(con)
		.await;
}

fn payment_processed_key_of(
	payment_summary_key: &str,
	payment_request: &PaymentProcessorRequest,
) -> String {
	format!("{payment_summary_key}:{}", payment_request.correlation_id)
}

async fn is_backend_failing_or_slow(
	health_key: &str,
	con: &mut redis::aio::MultiplexedConnection,
) -> bool {
	(con.hget(health_key, "failing").await.unwrap_or(1i32)) != 0
}

async fn is_already_processed(
	con: &mut redis::aio::MultiplexedConnection,
	payment: &Payment,
) -> bool {
	(con.sismember(
		PROCESSED_PAYMENTS_SET_KEY,
		payment.correlation_id.to_string(),
	)
	.await)
		.unwrap_or(false)
}
