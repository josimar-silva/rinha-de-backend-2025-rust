use actix_web::{HttpResponse, web};
use log::{error, info};
use redis::AsyncCommands;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct PaymentRequest {
	pub correlation_id: Uuid,
	pub amount:         f64,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct PaymentsSummaryResponse {
	pub default:  SummaryData,
	pub fallback: SummaryData,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct SummaryData {
	#[serde(rename = "totalRequests")]
	pub total_requests: i64,
	#[serde(rename = "totalAmount")]
	pub total_amount:   f64,
}

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

	let default_total_requests: i64 = con
		.hget("payments_summary_default", "totalRequests")
		.await
		.unwrap_or(0);
	let default_total_amount: f64 = con
		.hget("payments_summary_default", "totalAmount")
		.await
		.unwrap_or(0.0);

	let fallback_total_requests: i64 = con
		.hget("payments_summary_fallback", "totalRequests")
		.await
		.unwrap_or(0);
	let fallback_total_amount: f64 = con
		.hget("payments_summary_fallback", "totalAmount")
		.await
		.unwrap_or(0.0);

	let response = PaymentsSummaryResponse {
		default:  SummaryData {
			total_requests: default_total_requests,
			total_amount:   default_total_amount,
		},
		fallback: SummaryData {
			total_requests: fallback_total_requests,
			total_amount:   fallback_total_amount,
		},
	};

	HttpResponse::Ok().json(response)
}
