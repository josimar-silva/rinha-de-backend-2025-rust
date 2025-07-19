use std::ops::{Add, Sub};

use time::OffsetDateTime;

use crate::domain::repository::PaymentRepository;
use crate::use_cases::dto::{
	GetPaymentSummaryQuery, PaymentSummaryResult, PaymentsSummaryResponse,
};

#[derive(Clone)]
pub struct GetPaymentSummaryUseCase<R: PaymentRepository> {
	payment_repo: R,
}

impl<R: PaymentRepository> GetPaymentSummaryUseCase<R> {
	pub fn new(payment_repo: R) -> Self {
		Self { payment_repo }
	}

	pub async fn execute(
		&self,
		query: GetPaymentSummaryQuery,
	) -> Result<PaymentsSummaryResponse, Box<dyn std::error::Error + Send>> {
		let from = query
			.from
			.unwrap_or(OffsetDateTime::now_utc().sub(time::Duration::days(30)));
		let to = query
			.to
			.unwrap_or(OffsetDateTime::now_utc().add(time::Duration::days(30)));

		let (default_total_requests, default_total_amount) = self
			.payment_repo
			.get_summary_by_group("default", from, to)
			.await?;

		let (fallback_total_requests, fallback_total_amount) = self
			.payment_repo
			.get_summary_by_group("fallback", from, to)
			.await?;

		Ok(PaymentsSummaryResponse {
			default:  PaymentSummaryResult {
				total_requests: default_total_requests,
				total_amount:   default_total_amount,
			},
			fallback: PaymentSummaryResult {
				total_requests: fallback_total_requests,
				total_amount:   fallback_total_amount,
			},
		})
	}
}
