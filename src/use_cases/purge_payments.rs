use std::error::Error;

use crate::domain::repository::PaymentRepository;

#[derive(Clone)]
pub struct PurgePaymentsUseCase<R: PaymentRepository> {
	repository: R,
}

impl<R: PaymentRepository> PurgePaymentsUseCase<R> {
	pub fn new(repository: R) -> Self {
		Self { repository }
	}

	pub async fn execute(&self) -> Result<(), Box<dyn Error + Send>> {
		self.repository.clear().await
	}
}
