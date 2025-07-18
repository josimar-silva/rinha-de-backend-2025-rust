use actix_web::{HttpResponse, Responder, post, web};
use log::info;

use crate::infrastructure::persistence::redis_payment_repository::RedisPaymentRepository;
use crate::use_cases::purge_payments::PurgePaymentsUseCase;

#[post("/purge-payments")]
pub async fn payments_purge(
	purge_use_case: web::Data<PurgePaymentsUseCase<RedisPaymentRepository>>,
) -> impl Responder {
	info!("Received request to purge payments");
	match purge_use_case.execute().await {
		Ok(_) => {
			info!("Payments purged successfully");
			HttpResponse::Ok().body("Payments purged successfully")
		}
		Err(e) => {
			log::error!("Failed to purge payments: {e}");
			HttpResponse::InternalServerError()
				.body(format!("Failed to purge payments: {e}"))
		}
	}
}
