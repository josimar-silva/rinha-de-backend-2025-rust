use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Payment {
	pub correlation_id: Uuid,
	pub amount:         f64,
	#[serde(
		with = "time::serde::rfc3339::option",
		skip_serializing_if = "Option::is_none",
		default
	)]
	pub requested_at:   Option<OffsetDateTime>,
	#[serde(
		with = "time::serde::rfc3339::option",
		skip_serializing_if = "Option::is_none",
		default
	)]
	pub processed_at:   Option<OffsetDateTime>,
	#[serde(skip_serializing_if = "Option::is_none", default)]
	pub processed_by:   Option<String>,
}
