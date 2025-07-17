use actix_web::{HttpResponse, Responder, ResponseError, get, web};
use redis::{Connection, RedisResult, Script};

use super::errors::ApiError;
use super::schema::{PaymentsSummaryFilter, PaymentsSummaryResponse, SummaryData};
use crate::config::PROCESSED_PAYMENTS_SET_KEY;

#[get("/payments-summary")]
pub async fn payments_summary(
	redis_client: web::Data<redis::Client>,
	filter: web::Query<PaymentsSummaryFilter>,
) -> impl Responder {
	let mut con = match redis_client.get_connection() {
		Ok(con) => con,
		Err(_e) => {
			return ApiError::DatabaseConnectionError.error_response();
		}
	};

	let from = filter.from.map(|dt| dt.timestamp()).unwrap_or(i64::MIN);
	let to = filter.to.map(|dt| dt.timestamp()).unwrap_or(i64::MAX);

	let (default_total_requests, default_total_amount) =
		match payments_summary_lua(&mut con, "default", from, to) {
			Ok((req, amt)) => (req as i64, amt),
			Err(_e) => {
				return ApiError::TransactionError.error_response();
			}
		};

	let (fallback_total_requests, fallback_total_amount) =
		match payments_summary_lua(&mut con, "fallback", from, to) {
			Ok((req, amt)) => (req as i64, amt),
			Err(_e) => {
				return ApiError::TransactionError.error_response();
			}
		};

	HttpResponse::Ok().json(PaymentsSummaryResponse {
		default:  SummaryData {
			total_requests: default_total_requests,
			total_amount:   default_total_amount,
		},
		fallback: SummaryData {
			total_requests: fallback_total_requests,
			total_amount:   fallback_total_amount,
		},
	})
}

fn payments_summary_lua(
	conn: &mut Connection,
	group: &str,
	from_ts: i64,
	to_ts: i64,
) -> RedisResult<(usize, f64)> {
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

        return {total_requests, total_amount}
    "#,
	);

	let response: (i64, f64) = lua
		.key(PROCESSED_PAYMENTS_SET_KEY)
		.arg(from_ts)
		.arg(to_ts)
		.arg(format!("payment_summary:{group}"))
		.invoke(conn)?;

	Ok((response.0 as usize, response.1))
}
