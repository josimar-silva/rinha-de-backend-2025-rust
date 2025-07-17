use reqwest::Client;

use crate::domain::health_status::HealthStatus;
use crate::domain::payment_processor::PaymentProcessor;
use crate::domain::repository::PaymentProcessorRepository;

pub struct HealthCheckUseCase<R: PaymentProcessorRepository> {
	processor_repo: R,
	http_client:    Client,
}

impl<R: PaymentProcessorRepository> HealthCheckUseCase<R> {
	pub fn new(processor_repo: R, http_client: Client) -> Self {
		Self {
			processor_repo,
			http_client,
		}
	}

	pub async fn execute(
		&self,
		processor_name: String,
		processor_url: String,
	) -> Result<(), Box<dyn std::error::Error + Send>> {
		let health_url = format!("{processor_url}/payments/service-health");
		let processor = match self.http_client.get(&health_url).send().await {
			Ok(resp) => {
				if resp.status().is_success() {
					PaymentProcessor {
						name:              processor_name,
						url:               processor_url,
						health:            HealthStatus::Healthy,
						min_response_time: 100,
					}
				} else {
					PaymentProcessor {
						name:              processor_name,
						url:               processor_url,
						health:            HealthStatus::Failing,
						min_response_time: 100,
					}
				}
			}
			Err(_) => PaymentProcessor {
				name:              processor_name,
				url:               processor_url,
				health:            HealthStatus::Failing,
				min_response_time: 100,
			},
		};

		self.processor_repo.save(processor).await
	}
}
