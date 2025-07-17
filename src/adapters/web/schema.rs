use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct PaymentRequest {
	#[serde(rename = "correlationId")]
	pub correlation_id: Uuid,
	pub amount:         f64,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct PaymentResponse {
	pub payment: PaymentRequest,
	pub status:  String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct PaymentsSummaryFilter {
	#[serde(with = "chrono::serde::ts_seconds_option", default)]
	pub from: Option<DateTime<Utc>>,
	#[serde(with = "chrono::serde::ts_seconds_option", default)]
	pub to:   Option<DateTime<Utc>>,
}
