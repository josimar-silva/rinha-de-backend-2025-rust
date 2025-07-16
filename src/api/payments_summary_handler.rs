use actix_web::{HttpResponse, web};
use log::error;
use redis::AsyncCommands;

use super::schema::{PaymentsSummaryResponse, SummaryData};

async fn fetch_summary_data(
	con: &mut redis::aio::MultiplexedConnection,
	key: &str,
) -> (i64, f64) {
	let total_requests: i64 = con.hget(key, "totalRequests").await.unwrap_or(0);
	let total_amount: f64 = con.hget(key, "totalAmount").await.unwrap_or(0.0);
	(total_requests, total_amount)
}

pub async fn payments_summary(
	redis_client: web::Data<redis::Client>,
) -> HttpResponse {
	let mut con = match redis_client.get_multiplexed_async_connection().await {
		Ok(con) => con,
		Err(e) => {
			error!("Failed to get Redis connection for summary: {e}");
			return HttpResponse::InternalServerError()
				.body("Internal Server Error");
		}
	};

	let (default_total_requests, default_total_amount) =
		fetch_summary_data(&mut con, "payments_summary_default").await;
	let (fallback_total_requests, fallback_total_amount) =
		fetch_summary_data(&mut con, "payments_summary_fallback").await;

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
