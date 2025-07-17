use async_trait::async_trait;
use redis::{AsyncCommands, Client};

use crate::domain::payment_processor::PaymentProcessor;
use crate::domain::repository::PaymentProcessorRepository;

#[derive(Clone)]
pub struct RedisPaymentProcessorRepository {
	client: Client,
}

impl RedisPaymentProcessorRepository {
	pub fn new(client: Client) -> Self {
		Self { client }
	}
}

#[async_trait]
impl PaymentProcessorRepository for RedisPaymentProcessorRepository {
	async fn save(
		&self,
		processor: PaymentProcessor,
	) -> Result<(), Box<dyn std::error::Error + Send>> {
		let mut con = self
			.client
			.get_multiplexed_async_connection()
			.await
			.map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send>)?;

		let failing_status = match processor.health {
			crate::domain::health_status::HealthStatus::Healthy => 0,
			crate::domain::health_status::HealthStatus::Failing => 1,
			crate::domain::health_status::HealthStatus::Slow => 1,
		};

		let _: () = con
			.hset_multiple(format!("health:{}", processor.name), &[
				("failing", failing_status.to_string()),
				("min_response_time", processor.min_response_time.to_string()),
			])
			.await
			.map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send>)?;

		Ok(())
	}

	async fn get_health_of(
		&self,
		processor_name: &str,
	) -> Result<i32, Box<dyn std::error::Error + Send>> {
		let mut con = self
			.client
			.get_multiplexed_async_connection()
			.await
			.map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send>)?;

		let failing: String = con
			.hget(format!("health:{processor_name}"), "failing")
			.await
			.map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send>)
			.unwrap_or("1".to_string());

		let failing_status = failing.parse::<i32>().unwrap_or(1);

		Ok(failing_status)
	}
}
