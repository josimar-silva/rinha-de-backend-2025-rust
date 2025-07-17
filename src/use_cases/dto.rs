use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct CreatePaymentCommand {
	pub correlation_id: Uuid,
	pub amount:         f64,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct GetPaymentSummaryQuery {
	pub from: Option<i64>,
	pub to:   Option<i64>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct PaymentSummaryResult {
	pub total_requests: usize,
	pub total_amount:   f64,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct PaymentsSummaryResponse {
	pub default:  PaymentSummaryResult,
	pub fallback: PaymentSummaryResult,
}
