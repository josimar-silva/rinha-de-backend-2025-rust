use async_trait::async_trait;

#[async_trait]
pub trait PaymentRouter: Send + Sync + 'static {
	async fn get_processor_for_payment(&self) -> Option<(String, String)>;
}
