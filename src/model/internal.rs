use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Payment {
	pub correlation_id: Uuid,
	pub amount:         f64,
}
