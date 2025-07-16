use actix_web::{HttpResponse, Responder, ResponseError, get, web};
use redis::AsyncCommands;

use super::errors::ApiError;
use super::schema::{PaymentsSummaryResponse, SummaryData};

async fn fetch_summary_data(
	key: &str,
	con: &mut redis::aio::MultiplexedConnection,
) -> (i64, f64) {
	let total_requests: i64 = con.hget(key, "totalRequests").await.unwrap_or(0);
	let total_amount: f64 = con.hget(key, "totalAmount").await.unwrap_or(0.0);
	(total_requests, total_amount)
}

#[get("/payments-summary")]
pub async fn payments_summary(
	redis_client: web::Data<redis::Client>,
) -> impl Responder {
	let mut con = match redis_client.get_multiplexed_async_connection().await {
		Ok(con) => con,
		Err(_e) => {
			return ApiError::DatabaseConnectionError.error_response();
		}
	};

	let (default_total_requests, default_total_amount) =
		fetch_summary_data("payments_summary_default", &mut con).await;
	let (fallback_total_requests, fallback_total_amount) =
		fetch_summary_data("payments_summary_fallback", &mut con).await;

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
