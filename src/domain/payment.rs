use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Payment {
	pub correlation_id: Uuid,
	pub amount:         f64,
	#[serde(
		with = "chrono::serde::ts_seconds_option",
		skip_serializing_if = "Option::is_none",
		default
	)]
	pub requested_at:   Option<DateTime<Utc>>,
	#[serde(
		with = "chrono::serde::ts_seconds_option",
		skip_serializing_if = "Option::is_none",
		default
	)]
	pub processed_at:   Option<DateTime<Utc>>,
	#[serde(skip_serializing_if = "Option::is_none", default)]
	pub processed_by:   Option<String>,
}
