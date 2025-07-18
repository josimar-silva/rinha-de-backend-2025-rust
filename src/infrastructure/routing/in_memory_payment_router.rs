use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use async_trait::async_trait;

use crate::domain::payment_processor::PaymentProcessor;
use crate::domain::payment_router::PaymentRouter;

#[derive(Clone)]
pub struct InMemoryPaymentRouter {
	processors: Arc<RwLock<HashMap<String, PaymentProcessor>>>,
}

impl InMemoryPaymentRouter {
	pub fn new() -> Self {
		Self {
			processors: Arc::new(RwLock::new(HashMap::new())),
		}
	}

	pub fn update_processor_health(&self, processor: PaymentProcessor) {
		let mut processors = self.processors.write().unwrap();
		processors.insert(processor.name.clone(), processor);
	}
}

impl Default for InMemoryPaymentRouter {
	fn default() -> Self {
		Self::new()
	}
}

#[async_trait]
impl PaymentRouter for InMemoryPaymentRouter {
	async fn get_processor_for_payment(&self) -> Option<(String, String)> {
		let processors = self.processors.read().unwrap();

		// Prioritize default if healthy and fast
		if let Some(default_processor) = processors.get("default") &&
			default_processor.health.is_healthy() &&
			default_processor.min_response_time < 100
		{
			return Some((
				default_processor.url.clone(),
				default_processor.name.clone(),
			));
		}

		// Fallback to fallback processor if healthy and fast
		if let Some(fallback_processor) = processors.get("fallback") &&
			fallback_processor.health.is_healthy() &&
			fallback_processor.min_response_time < 100
		{
			return Some((
				fallback_processor.url.clone(),
				fallback_processor.name.clone(),
			));
		}

		None
	}
}
