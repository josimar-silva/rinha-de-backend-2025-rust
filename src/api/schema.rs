use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct PaymentRequest {
	pub correlation_id: Uuid,
	pub amount:         f64,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct PaymentsSummaryResponse {
	pub default:  SummaryData,
	pub fallback: SummaryData,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct SummaryData {
	#[serde(rename = "totalRequests")]
	pub total_requests: i64,
	#[serde(rename = "totalAmount")]
	pub total_amount:   f64,
}
