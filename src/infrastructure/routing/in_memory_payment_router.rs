use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use async_trait::async_trait;
use circuitbreaker_rs::{CircuitBreaker, DefaultPolicy};

use crate::domain::payment_processor::PaymentProcessor;
use crate::domain::payment_router::PaymentRouter;
use crate::use_cases::process_payment::PaymentProcessingError;

#[derive(Clone)]
pub struct InMemoryPaymentRouter {
	pub processors:       Arc<RwLock<HashMap<String, PaymentProcessor>>>,
	pub default_breaker:  CircuitBreaker<DefaultPolicy, PaymentProcessingError>,
	pub fallback_breaker: CircuitBreaker<DefaultPolicy, PaymentProcessingError>,
}

impl InMemoryPaymentRouter {
	pub fn new() -> Self {
		Self {
			processors:       Arc::new(RwLock::new(HashMap::new())),
			default_breaker:
				CircuitBreaker::<DefaultPolicy, PaymentProcessingError>::builder()
					.build(),
			fallback_breaker:
				CircuitBreaker::<DefaultPolicy, PaymentProcessingError>::builder()
					.build(),
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
	async fn get_processor_for_payment(
		&self,
	) -> Option<(
		String,
		String,
		CircuitBreaker<DefaultPolicy, PaymentProcessingError>,
	)> {
		let processors = self.processors.read().unwrap();

		if let Some(default_processor) = processors.get("default") &&
			default_processor.health.is_healthy() &&
			default_processor.min_response_time < 100 &&
			!matches!(
				self.default_breaker.current_state(),
				circuitbreaker_rs::State::Open
			) {
			return Some((
				default_processor.url.clone(),
				default_processor.name.clone(),
				self.default_breaker.clone(),
			));
		}

		if let Some(fallback_processor) = processors.get("fallback") &&
			fallback_processor.health.is_healthy() &&
			fallback_processor.min_response_time < 100 &&
			!matches!(
				self.fallback_breaker.current_state(),
				circuitbreaker_rs::State::Open
			) {
			return Some((
				fallback_processor.url.clone(),
				fallback_processor.name.clone(),
				self.fallback_breaker.clone(),
			));
		}

		None
	}
}

#[cfg(test)]
mod tests {

	use circuitbreaker_rs::State;
	use rinha_de_backend::domain::health_status::HealthStatus;
	use rinha_de_backend::domain::payment_processor::PaymentProcessor;
	use rinha_de_backend::domain::payment_router::PaymentRouter;
	use rinha_de_backend::infrastructure::routing::in_memory_payment_router::InMemoryPaymentRouter;

	#[tokio::test]
	async fn test_get_processor_for_payment_default_healthy() {
		let router = InMemoryPaymentRouter::new();
		let default_processor = PaymentProcessor {
			name:              "default".to_string(),
			url:               "http://default.com".to_string(),
			health:            HealthStatus::Healthy,
			min_response_time: 50,
		};
		router.update_processor_health(default_processor.clone());

		let (url, name, breaker) = router.get_processor_for_payment().await.unwrap();
		assert_eq!(url, default_processor.url);
		assert_eq!(name, default_processor.name);
		assert_eq!(breaker.current_state(), State::Closed);
	}

	#[tokio::test]
	async fn test_get_processor_for_payment_default_unhealthy() {
		let router = InMemoryPaymentRouter::new();
		let default_processor = PaymentProcessor {
			name:              "default".to_string(),
			url:               "http://default.com".to_string(),
			health:            HealthStatus::Failing,
			min_response_time: 50,
		};
		router.update_processor_health(default_processor.clone());

		let result = router.get_processor_for_payment().await;
		assert!(result.is_none());
	}

	#[tokio::test]
	async fn test_get_processor_for_payment_default_slow() {
		let router = InMemoryPaymentRouter::new();
		let default_processor = PaymentProcessor {
			name:              "default".to_string(),
			url:               "http://default.com".to_string(),
			health:            HealthStatus::Healthy,
			min_response_time: 150, // Too slow
		};
		router.update_processor_health(default_processor.clone());

		let result = router.get_processor_for_payment().await;
		assert!(result.is_none());
	}

	#[tokio::test]
	async fn test_get_processor_for_payment_default_circuit_open() {
		let router = InMemoryPaymentRouter::new();
		let default_processor = PaymentProcessor {
			name:              "default".to_string(),
			url:               "http://default.com".to_string(),
			health:            HealthStatus::Healthy,
			min_response_time: 50,
		};
		router.update_processor_health(default_processor.clone());

		router.default_breaker.force_open();

		let result = router.get_processor_for_payment().await;
		assert!(result.is_none());
	}

	#[tokio::test]
	async fn test_get_processor_for_payment_fallback_healthy() {
		let router = InMemoryPaymentRouter::new();
		let fallback_processor = PaymentProcessor {
			name:              "fallback".to_string(),
			url:               "http://fallback.com".to_string(),
			health:            HealthStatus::Healthy,
			min_response_time: 50,
		};
		router.update_processor_health(fallback_processor.clone());

		// Ensure default is not chosen
		let default_processor = PaymentProcessor {
			name:              "default".to_string(),
			url:               "http://default.com".to_string(),
			health:            HealthStatus::Failing, // Make default unhealthy
			min_response_time: 50,
		};
		router.update_processor_health(default_processor.clone());

		let (url, name, breaker) = router.get_processor_for_payment().await.unwrap();
		assert_eq!(url, fallback_processor.url);
		assert_eq!(name, fallback_processor.name);
		assert_eq!(breaker.current_state(), State::Closed);
	}

	#[tokio::test]
	async fn test_get_processor_for_payment_no_processors() {
		let router = InMemoryPaymentRouter::new();
		let result = router.get_processor_for_payment().await;
		assert!(result.is_none());
	}

	#[tokio::test]
	async fn test_update_processor_health() {
		let router = InMemoryPaymentRouter::new();
		let processor = PaymentProcessor {
			name:              "test_processor".to_string(),
			url:               "http://test.com".to_string(),
			health:            HealthStatus::Healthy,
			min_response_time: 100,
		};
		router.update_processor_health(processor.clone());

		let processors = router.processors.read().unwrap();
		assert!(processors.contains_key("test_processor"));
		assert_eq!(processors["test_processor"].url, processor.url);
	}
}
