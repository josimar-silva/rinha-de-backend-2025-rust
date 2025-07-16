use actix_web::{HttpResponse, web};
use log::{error, info};
use redis::AsyncCommands;

use super::schema::PaymentRequest;

pub async fn payments(
	req: web::Json<PaymentRequest>,
	redis_client: web::Data<redis::Client>,
) -> HttpResponse {
	let mut con = match redis_client.get_multiplexed_async_connection().await {
		Ok(con) => con,
		Err(e) => {
			error!("Failed to get Redis connection: {e}");
			return HttpResponse::InternalServerError()
				.body("Internal Server Error");
		}
	};

	let payment_json = match serde_json::to_string(&req.0) {
		Ok(json) => json,
		Err(e) => {
			error!("Failed to serialize payment request: {e}");
			return HttpResponse::InternalServerError()
				.body("Internal Server Error");
		}
	};

	match con
		.lpush::<&str, String, ()>("payments_queue", payment_json)
		.await
	{
		Ok(_) => {
			info!("Payment received and queued: {}", req.correlation_id);
			HttpResponse::Ok().body("Payment received")
		}
		Err(e) => {
			error!("Failed to push payment to Redis queue: {e}");
			HttpResponse::InternalServerError().body("Internal Server Error")
		}
	}
}
