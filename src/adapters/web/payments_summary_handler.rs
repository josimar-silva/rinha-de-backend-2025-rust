use actix_web::{HttpResponse, Responder, ResponseError, get, web};

use crate::adapters::web::errors::ApiError;
use crate::adapters::web::schema::PaymentsSummaryFilter;
use crate::use_cases::dto::GetPaymentSummaryQuery;
use crate::use_cases::get_payment_summary::GetPaymentSummaryUseCase;

#[get("/payments-summary")]
pub async fn payments_summary(
	filter: web::Query<PaymentsSummaryFilter>,
	get_payment_summary_use_case: web::Data<GetPaymentSummaryUseCase<crate::infrastructure::persistence::redis_payment_repository::RedisPaymentRepository>>,
) -> impl Responder {
	let query = GetPaymentSummaryQuery {
		from: filter.from.map(|dt| dt.timestamp()),
		to:   filter.to.map(|dt| dt.timestamp()),
	};

	match get_payment_summary_use_case.execute(query).await {
		Ok(summary) => HttpResponse::Ok().json(summary),
		Err(e) => {
			eprintln!("Error getting payment summary: {e:?}");
			ApiError::InternalServerError.error_response()
		}
	}
}
