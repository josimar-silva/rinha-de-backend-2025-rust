use async_trait::async_trait;
use circuitbreaker_rs::{CircuitBreaker, DefaultPolicy};

use crate::use_cases::process_payment::PaymentProcessingError;

#[async_trait]
pub trait PaymentRouter: Send + Sync + 'static {
	async fn get_processor_for_payment(
		&self,
	) -> Option<(
		String,
		String,
		CircuitBreaker<DefaultPolicy, PaymentProcessingError>,
	)>;
}
