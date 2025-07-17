use chrono::Utc;
use log::error;
use reqwest::Client;

use crate::domain::payment::Payment;
use crate::domain::repository::PaymentRepository;

#[derive(Clone)]
pub struct ProcessPaymentUseCase<R: PaymentRepository> {
	payment_repo: R,
	http_client:  Client,
}

impl<R: PaymentRepository> ProcessPaymentUseCase<R> {
	pub fn new(payment_repo: R, http_client: Client) -> Self {
		Self {
			payment_repo,
			http_client,
		}
	}

	pub async fn execute(
		&self,
		mut payment: Payment,
		processor_url: String,
		processed_by: String,
	) -> Result<bool, Box<dyn std::error::Error + Send>> {
		match self
			.http_client
			.post(format!("{processor_url}/payments"))
			.json(&payment)
			.send()
			.await
		{
			Ok(resp) => {
				if resp.status().is_success() {
					payment.requested_at = Some(Utc::now());
					payment.processed_at = Some(Utc::now());
					payment.processed_by = Some(processed_by);
					self.payment_repo.save(payment).await?;
					Ok(true)
				} else {
					error!(
						"Processor returned non-success status for {}: {}",
						payment.correlation_id,
						resp.status()
					);
					Ok(false)
				}
			}
			Err(e) => {
				error!(
					"Failed to send payment {} to processor: {e}",
					payment.correlation_id
				);
				Ok(false)
			}
		}
	}
}
