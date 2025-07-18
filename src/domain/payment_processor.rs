use crate::domain::health_status::HealthStatus;

#[derive(Clone)]
pub struct PaymentProcessor {
	pub name:              String,
	pub url:               String,
	pub health:            HealthStatus,
	pub min_response_time: u64,
}
