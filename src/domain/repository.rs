use async_trait::async_trait;
use time::OffsetDateTime;

use crate::domain::payment::Payment;

#[async_trait]
pub trait PaymentRepository: Send + Sync + 'static {
	async fn save(
		&self,
		payment: Payment,
	) -> Result<(), Box<dyn std::error::Error + Send>>;
	async fn get_summary_by_group(
		&self,
		group: &str,
		from_ts: OffsetDateTime,
		to_ts: OffsetDateTime,
	) -> Result<(usize, f64), Box<dyn std::error::Error + Send>>;
	async fn get_payment_summary(
		&self,
		group: &str,
		payment_id: &str,
	) -> Result<Payment, Box<dyn std::error::Error + Send>>;
	async fn is_already_processed(
		&self,
		payment_id: &str,
	) -> Result<bool, Box<dyn std::error::Error + Send>>;
	async fn clear(&self) -> Result<(), Box<dyn std::error::Error + Send>>;
}
