use actix_web::cookie::time::OffsetDateTime;
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
	#[serde(with = "time::serde::rfc3339::option", default)]
	pub from: Option<OffsetDateTime>,
	#[serde(with = "time::serde::rfc3339::option", default)]
	pub to:   Option<OffsetDateTime>,
}
