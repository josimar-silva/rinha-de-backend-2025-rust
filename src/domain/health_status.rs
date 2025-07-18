#[derive(Debug, Clone, PartialEq)]
pub enum HealthStatus {
	Healthy,
	Failing,
	Slow,
}

impl HealthStatus {
	pub fn is_healthy(&self) -> bool {
		matches!(self, HealthStatus::Healthy)
	}
}
