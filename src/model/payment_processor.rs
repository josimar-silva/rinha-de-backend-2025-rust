use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Deserialize, Serialize)]
pub struct PaymentProcessorRequest {
	pub correlation_id: Uuid,
	pub amount:         f64,
	#[serde(with = "chrono::serde::ts_seconds")]
	pub requested_at:   DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct HealthCheckResponse {
	pub failing:           bool,
	#[serde(rename = "minResponseTime")]
	pub min_response_time: u64,
}
