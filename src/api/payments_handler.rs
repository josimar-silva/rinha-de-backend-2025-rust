use actix_web::{HttpResponse, Responder, ResponseError, post, web};
use log::info;
use redis::AsyncCommands;

use super::errors::ApiError;
use super::schema::PaymentRequest;
use crate::api::schema::PaymentResponse;

#[post("/payments")]
pub async fn payments(
	payload: web::Json<PaymentRequest>,
	redis_client: web::Data<redis::Client>,
) -> impl Responder {
	let mut con = match redis_client.get_multiplexed_async_connection().await {
		Ok(con) => con,
		Err(_e) => {
			return ApiError::DatabaseConnectionError.error_response();
		}
	};

	let payment_json = match serde_json::to_string(&payload.0) {
		Ok(json) => json,
		Err(_e) => {
			return ApiError::BadClientDataError.error_response();
		}
	};

	match con
		.lpush::<&str, String, ()>("payments_queue", payment_json)
		.await
	{
		Ok(_) => {
			info!("Payment received and queued: {}", payload.correlation_id);
			HttpResponse::Ok().json(PaymentResponse {
				correlation_id: payload.correlation_id,
				amount:         payload.amount,
				status:         "queued".to_string(),
			})
		}
		Err(_e) => ApiError::TransactionError.error_response(),
	}
}
