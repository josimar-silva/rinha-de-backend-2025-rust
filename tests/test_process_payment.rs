use std::time::Duration;

use circuitbreaker_rs::{CircuitBreaker, DefaultPolicy};
use reqwest::Client;
use rinha_de_backend::domain::payment::Payment;
use rinha_de_backend::infrastructure::persistence::redis_payment_repository::RedisPaymentRepository;
use rinha_de_backend::use_cases::process_payment::{
	PaymentProcessingError, ProcessPaymentUseCase,
};
use uuid::Uuid;

mod support;

use crate::support::payment_processor_container::setup_payment_processors;
use crate::support::redis_container::get_test_redis_client;

#[tokio::test]
async fn test_process_payment_success() {
	let redis_container = get_test_redis_client().await;
	let redis_client = redis_container.client.clone();
	let payment_repo = RedisPaymentRepository::new(redis_client.clone());
	let (default_processor_container, _) = setup_payment_processors().await;
	let default_url = default_processor_container.url.clone();
	let http_client = Client::builder()
		.timeout(Duration::from_secs(1))
		.build()
		.unwrap();
	let process_payment_use_case =
		ProcessPaymentUseCase::new(payment_repo.clone(), http_client.clone());

	let payment = Payment {
		correlation_id: Uuid::new_v4(),
		amount:         100.0,
		requested_at:   None,
		processed_at:   None,
		processed_by:   None,
	};

	let circuit_breaker: CircuitBreaker<DefaultPolicy, PaymentProcessingError> =
		CircuitBreaker::<DefaultPolicy, PaymentProcessingError>::builder()
			.failure_threshold(0.5)
			.cooldown(Duration::from_secs(30))
			.build();

	let result = process_payment_use_case
		.execute(payment, default_url, "default".to_string(), circuit_breaker)
		.await;

	assert!(result.is_ok());
	assert!(result.unwrap());
}

#[tokio::test]
async fn test_process_payment_duplicate_returns_false() {
	let redis_container = get_test_redis_client().await;
	let redis_client = redis_container.client.clone();
	let payment_repo = RedisPaymentRepository::new(redis_client.clone());
	let (default_processor_container, _) = setup_payment_processors().await;
	let default_url = default_processor_container.url.clone();
	let http_client = Client::builder()
		.timeout(Duration::from_secs(1))
		.build()
		.unwrap();
	let process_payment_use_case =
		ProcessPaymentUseCase::new(payment_repo.clone(), http_client.clone());

	let payment = Payment {
		correlation_id: Uuid::new_v4(),
		amount:         100.0,
		requested_at:   None,
		processed_at:   None,
		processed_by:   None,
	};

	let circuit_breaker: CircuitBreaker<DefaultPolicy, PaymentProcessingError> =
		CircuitBreaker::<DefaultPolicy, PaymentProcessingError>::builder()
			.failure_threshold(0.5)
			.cooldown(Duration::from_secs(30))
			.build();

	// First attempt: should succeed
	let result1 = process_payment_use_case
		.execute(
			payment.clone(),
			default_url.clone(),
			"default".to_string(),
			circuit_breaker.clone(),
		)
		.await;

	assert!(result1.is_ok());
	assert!(result1.unwrap());

	// Second attempt with the same payment: should return false
	let result2 = process_payment_use_case
		.execute(payment, default_url, "default".to_string(), circuit_breaker)
		.await;

	assert!(result2.is_ok());
	let is_processed = result2.unwrap();
	assert!(!is_processed);
}

#[tokio::test]
async fn test_process_payment_500_returns_false() {
	let redis_container = get_test_redis_client().await;
	let redis_client = redis_container.client.clone();
	let payment_repo = RedisPaymentRepository::new(redis_client.clone());
	let (default_processor_container, _) = setup_payment_processors().await;
	let default_url = default_processor_container.url.clone();
	let http_client = Client::builder()
		.timeout(Duration::from_secs(1))
		.build()
		.unwrap();
	let process_payment_use_case =
		ProcessPaymentUseCase::new(payment_repo.clone(), http_client.clone());

	let payment = Payment {
		correlation_id: Uuid::new_v4(),
		amount:         100.0,
		requested_at:   None,
		processed_at:   None,
		processed_by:   None,
	};

	let circuit_breaker: CircuitBreaker<DefaultPolicy, PaymentProcessingError> =
		CircuitBreaker::<DefaultPolicy, PaymentProcessingError>::builder()
			.failure_threshold(0.5)
			.cooldown(Duration::from_secs(30))
			.build();

	// Configure the payment processor to return 500
	let admin_url = format!("{default_url}/admin/configurations/failure");
	let client = reqwest::Client::new();
	client
		.put(&admin_url)
		.header("X-Rinha-Token", "123")
		.json(&serde_json::json!({ "failure": true }))
		.send()
		.await
		.unwrap()
		.error_for_status()
		.unwrap();

	let result = process_payment_use_case
		.execute(payment, default_url, "default".to_string(), circuit_breaker)
		.await;

	assert!(result.is_err());
}

#[tokio::test]
async fn test_process_payment_circuit_breaker_open_returns_false() {
	let redis_container = get_test_redis_client().await;
	let redis_client = redis_container.client.clone();
	let payment_repo = RedisPaymentRepository::new(redis_client.clone());
	let (default_processor_container, _) = setup_payment_processors().await;
	let default_url = default_processor_container.url.clone();
	let http_client = Client::builder()
		.timeout(Duration::from_secs(1))
		.build()
		.unwrap();
	let process_payment_use_case =
		ProcessPaymentUseCase::new(payment_repo.clone(), http_client.clone());

	let payment = Payment {
		correlation_id: Uuid::new_v4(),
		amount:         100.0,
		requested_at:   None,
		processed_at:   None,
		processed_by:   None,
	};

	let circuit_breaker: CircuitBreaker<DefaultPolicy, PaymentProcessingError> =
		CircuitBreaker::<DefaultPolicy, PaymentProcessingError>::builder()
			.failure_threshold(0.5)
			.cooldown(Duration::from_secs(30))
			.build();

	// Manually open the circuit breaker
	circuit_breaker.force_open();

	let result = process_payment_use_case
		.execute(payment, default_url, "default".to_string(), circuit_breaker)
		.await;

	assert!(result.is_ok());
	assert!(!result.unwrap());
}
