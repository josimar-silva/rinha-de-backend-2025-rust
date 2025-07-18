use std::error::Error;
use std::fmt;

use chrono::Utc;
use circuitbreaker_rs::{BreakerError, CircuitBreaker, DefaultPolicy};
use log::error;
use reqwest::Client;

use crate::domain::payment::Payment;
use crate::domain::repository::PaymentRepository;

#[derive(Debug)]
pub struct PaymentProcessingError(String);

impl fmt::Display for PaymentProcessingError {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(f, "Service error: {}", self.0)
	}
}

impl Error for PaymentProcessingError {}

impl
	From<
		std::boxed::Box<
			dyn std::error::Error + std::marker::Send + std::marker::Sync + 'static,
		>,
	> for PaymentProcessingError
{
	fn from(
		err: Box<
			dyn std::error::Error + std::marker::Send + std::marker::Sync + 'static,
		>,
	) -> Self {
		PaymentProcessingError(err.to_string())
	}
}

#[derive(Clone)]
pub struct ProcessPaymentUseCase<R: PaymentRepository> {
	payment_repo:    R,
	http_client:     Client,
	circuit_breaker: CircuitBreaker<DefaultPolicy, PaymentProcessingError>,
}

impl<R: PaymentRepository> ProcessPaymentUseCase<R> {
	pub fn new(
		payment_repo: R,
		http_client: Client,
		circuit_breaker: CircuitBreaker<DefaultPolicy, PaymentProcessingError>,
	) -> Self {
		Self {
			payment_repo,
			http_client,
			circuit_breaker,
		}
	}

	pub async fn execute(
		&self,
		mut payment: Payment,
		processor_url: String,
		processed_by: String,
	) -> Result<bool, Box<dyn std::error::Error + Send>> {
		payment.requested_at = Some(Utc::now());

		let result = self
			.circuit_breaker
			.call_async(|| async {
				let resp = self
					.http_client
					.post(format!("{processor_url}/payments"))
					.json(&payment)
					.send()
					.await
					.map_err(|e| {
						Box::new(e) as Box<dyn std::error::Error + Send + Sync>
					})?;

				if resp.status().is_success() {
					Ok("Success".to_string())
				} else {
					error!(
						"Processor returned non-success status for {}: {}",
						payment.correlation_id,
						resp.status()
					);
					Err(PaymentProcessingError("Service unavailable".to_string()))
				}
			})
			.await;

		match result {
			Ok(_result) => {
				payment.processed_at = Some(Utc::now());
				payment.processed_by = Some(processed_by);
				self.payment_repo.save(payment).await?;
				Ok(true)
			}
			Err(BreakerError::Open) => Ok(false),
			Err(BreakerError::Operation(e)) => {
				error!("Circuit breaker prevented execution: {e}");
				Err(Box::new(e) as Box<dyn std::error::Error + Send>)
			}
			Err(e) => {
				error!("Operation failed: {e}");
				Err(Box::new(e) as Box<dyn std::error::Error + Send>)
			}
		}
	}
}
