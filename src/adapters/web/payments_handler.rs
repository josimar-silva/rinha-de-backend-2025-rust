use actix_web::{HttpResponse, Responder, ResponseError, post, web};
use log::{info, warn};

use crate::adapters::web::errors::ApiError;
use crate::adapters::web::schema::{PaymentRequest, PaymentResponse};
use crate::use_cases::create_payment::CreatePaymentUseCase;
use crate::use_cases::dto::CreatePaymentCommand;

#[post("/payments")]
pub async fn payments(
	payload: web::Json<PaymentRequest>,
	create_payment_use_case: web::Data<
		CreatePaymentUseCase<
			crate::infrastructure::queue::redis_payment_queue::PaymentQueue,
		>,
	>,
) -> impl Responder {
	let command = CreatePaymentCommand {
		correlation_id: payload.correlation_id,
		amount:         payload.amount,
	};

	match create_payment_use_case.execute(command).await {
		Ok(_) => {
			info!("Payment received and queued: {}", payload.correlation_id);
			HttpResponse::Ok().json(PaymentResponse {
				payment: payload.0,
				status:  "queued".to_string(),
			})
		}
		Err(e) => {
			warn!("Error processing payment: {e:?}");
			ApiError::InternalServerError.error_response()
		}
	}
}
