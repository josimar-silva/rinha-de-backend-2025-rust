use chrono::Days;
use circuitbreaker_rs::{CircuitBreaker, DefaultPolicy};
use reqwest::Client;
use rinha_de_backend::domain::health_status::HealthStatus;
use rinha_de_backend::domain::payment::Payment;
use rinha_de_backend::domain::payment_processor::PaymentProcessor;
use rinha_de_backend::domain::queue::{Message, Queue};
use rinha_de_backend::domain::repository::PaymentRepository;
use rinha_de_backend::infrastructure::persistence::redis_payment_repository::RedisPaymentRepository;
use rinha_de_backend::infrastructure::queue::redis_payment_queue::PaymentQueue;
use rinha_de_backend::infrastructure::routing::in_memory_payment_router::InMemoryPaymentRouter;
use rinha_de_backend::infrastructure::workers::payment_processor_worker::payment_processing_worker;
use rinha_de_backend::use_cases::process_payment::{
	PaymentProcessingError, ProcessPaymentUseCase,
};
use tokio::time::Duration;
use uuid::Uuid;

mod support;

use crate::support::payment_processor_container::setup_payment_processors;
use crate::support::redis_container::get_test_redis_client;

#[tokio::test]
async fn test_payment_processing_worker_default_success() {
	let redis_container = get_test_redis_client().await;
	let redis_client = redis_container.client.clone();
	let (default_processor_container, fallback_processor_container) =
		setup_payment_processors().await;
	let default_url = default_processor_container.url.clone();
	let fallback_url = fallback_processor_container.url.clone();
	let http_client = Client::new();
	let breaker =
		CircuitBreaker::<DefaultPolicy, PaymentProcessingError>::builder().build();
	let redis_queue = PaymentQueue::new(redis_client.clone());
	let payment_repo = RedisPaymentRepository::new(redis_client.clone());
	let process_payment_use_case = ProcessPaymentUseCase::new(
		payment_repo.clone(),
		http_client.clone(),
		breaker,
	);
	let router = InMemoryPaymentRouter::new();

	// Set up processor health
	let default_processor = PaymentProcessor {
		name:              "default".to_string(),
		url:               default_url.clone(),
		health:            HealthStatus::Healthy,
		min_response_time: 0,
	};
	router.update_processor_health(default_processor);

	let fallback_processor = PaymentProcessor {
		name:              "fallback".to_string(),
		url:               fallback_url.clone(),
		health:            HealthStatus::Failing,
		min_response_time: 0,
	};
	router.update_processor_health(fallback_processor);

	let payment_to_process = Payment {
		correlation_id: Uuid::new_v4(),
		amount:         250.0,
		requested_at:   None,
		processed_at:   None,
		processed_by:   None,
	};

	// Push payment to queue
	redis_queue
		.push(Message {
			id:   Uuid::new_v4(),
			body: payment_to_process.clone(),
		})
		.await
		.unwrap();

	let worker_handle = tokio::spawn(payment_processing_worker(
		redis_queue.clone(),
		payment_repo.clone(),
		process_payment_use_case.clone(),
		router.clone(),
	));

	// Give the worker some time to process the payment
	tokio::time::sleep(Duration::from_secs(10)).await;

	let processed_payment = payment_repo
		.get_payment_summary(
			"default",
			&payment_to_process.correlation_id.to_string(),
		)
		.await
		.unwrap();

	assert_eq!(processed_payment.amount, 250.0);
	assert!(processed_payment.processed_by.is_some());
	assert_eq!(processed_payment.processed_by.unwrap(), "default");

	// Abort the worker to clean up
	worker_handle.abort();
}

#[tokio::test]
async fn test_payment_processing_worker_fallback_success() {
	let redis_container = get_test_redis_client().await;
	let redis_client = redis_container.client.clone();
	let (default_processor_container, fallback_processor_container) =
		setup_payment_processors().await;
	let default_url = default_processor_container.url.clone();
	let fallback_url = fallback_processor_container.url.clone();
	let http_client = Client::new();
	let breaker =
		CircuitBreaker::<DefaultPolicy, PaymentProcessingError>::builder().build();
	let payment_queue = PaymentQueue::new(redis_client.clone());
	let payment_repo = RedisPaymentRepository::new(redis_client.clone());
	let process_payment_use_case = ProcessPaymentUseCase::new(
		payment_repo.clone(),
		http_client.clone(),
		breaker,
	);
	let router = InMemoryPaymentRouter::new();

	// Set up processor health
	let default_processor = PaymentProcessor {
		name:              "default".to_string(),
		url:               default_url.clone(),
		health:            HealthStatus::Failing,
		min_response_time: 10000,
	};
	router.update_processor_health(default_processor);

	let fallback_processor = PaymentProcessor {
		name:              "fallback".to_string(),
		url:               fallback_url.clone(),
		health:            HealthStatus::Healthy,
		min_response_time: 10,
	};
	router.update_processor_health(fallback_processor);

	let payment_to_process = Payment {
		correlation_id: Uuid::new_v4(),
		amount:         300.0,
		requested_at:   None,
		processed_at:   None,
		processed_by:   None,
	};

	payment_queue
		.push(Message {
			id:   Uuid::new_v4(),
			body: payment_to_process.clone(),
		})
		.await
		.unwrap();

	let worker_handle = tokio::spawn(payment_processing_worker(
		payment_queue.clone(),
		payment_repo.clone(),
		process_payment_use_case.clone(),
		router.clone(),
	));

	// Give the worker some time to process the payment
	tokio::time::sleep(Duration::from_secs(10)).await;

	let processed_payment = payment_repo
		.get_payment_summary(
			"fallback",
			&payment_to_process.correlation_id.to_string(),
		)
		.await
		.unwrap();

	assert_eq!(processed_payment.amount, 300.0);
	assert_eq!(processed_payment.processed_by.unwrap(), "fallback");

	// Abort the worker to clean up
	worker_handle.abort();
}

#[tokio::test]
async fn test_payment_processing_worker_requeue_message_given_processor_are_down() {
	let redis_container = get_test_redis_client().await;
	let redis_client = redis_container.client.clone();
	let http_client = Client::new();
	let breaker =
		CircuitBreaker::<DefaultPolicy, PaymentProcessingError>::builder().build();
	let redis_queue = PaymentQueue::new(redis_client.clone());
	let payment_repo = RedisPaymentRepository::new(redis_client.clone());
	let process_payment_use_case = ProcessPaymentUseCase::new(
		payment_repo.clone(),
		http_client.clone(),
		breaker,
	);
	let router = InMemoryPaymentRouter::new();

	// Set up processors to be failing
	let default_processor = PaymentProcessor {
		name:              "default".to_string(),
		url:               "http://non-existent-url:8080".to_string(),
		health:            HealthStatus::Failing,
		min_response_time: 0,
	};
	router.update_processor_health(default_processor);

	let fallback_processor = PaymentProcessor {
		name:              "fallback".to_string(),
		url:               "http://non-existent-url:8080".to_string(),
		health:            HealthStatus::Failing,
		min_response_time: 0,
	};
	router.update_processor_health(fallback_processor);

	let payment_to_process = Payment {
		correlation_id: Uuid::new_v4(),
		amount:         400.0,
		requested_at:   None,
		processed_at:   None,
		processed_by:   None,
	};

	// Push payment to queue
	redis_queue
		.push(Message::with(
			payment_to_process.correlation_id,
			payment_to_process.clone(),
		))
		.await
		.unwrap();

	let worker_handle = tokio::spawn(payment_processing_worker(
		redis_queue.clone(),
		payment_repo.clone(),
		process_payment_use_case.clone(),
		router.clone(),
	));

	// Give the worker some time to attempt processing and re-queue
	tokio::time::sleep(Duration::from_secs(5)).await;

	// Verify payment is re-queued
	let message = redis_queue.pop().await.unwrap().unwrap();
	let deserialized_payment: Payment = message.body;

	assert_eq!(
		deserialized_payment.correlation_id,
		payment_to_process.correlation_id
	);
	assert_eq!(deserialized_payment.amount, payment_to_process.amount);

	// Abort the worker to clean up
	worker_handle.abort();
}

#[tokio::test]
async fn test_payment_processing_worker_skip_processed_message() {
	let redis_container = get_test_redis_client().await;
	let redis_client = redis_container.client.clone();
	let (default_processor_container, fallback_processor_container) =
		setup_payment_processors().await;
	let default_url = default_processor_container.url.clone();
	let fallback_url = fallback_processor_container.url.clone();
	let http_client = Client::new();
	let breaker =
		CircuitBreaker::<DefaultPolicy, PaymentProcessingError>::builder().build();
	let redis_queue = PaymentQueue::new(redis_client.clone());
	let payment_repo = RedisPaymentRepository::new(redis_client.clone());
	let process_payment_use_case = ProcessPaymentUseCase::new(
		payment_repo.clone(),
		http_client.clone(),
		breaker,
	);
	let router = InMemoryPaymentRouter::new();

	// Set up processor health
	let default_processor = PaymentProcessor {
		name:              "default".to_string(),
		url:               default_url.clone(),
		health:            HealthStatus::Healthy,
		min_response_time: 0,
	};
	router.update_processor_health(default_processor);

	let fallback_processor = PaymentProcessor {
		name:              "fallback".to_string(),
		url:               fallback_url.clone(),
		health:            HealthStatus::Failing,
		min_response_time: 0,
	};
	router.update_processor_health(fallback_processor);

	let payment_to_process = Payment {
		correlation_id: Uuid::new_v4(),
		amount:         500.0,
		requested_at:   None,
		processed_at:   None,
		processed_by:   None,
	};

	// Pre-process the payment to simulate it being already processed
	let pre_processed_payment = Payment {
		correlation_id: payment_to_process.correlation_id,
		amount:         payment_to_process.amount,
		requested_at:   Some(chrono::Utc::now()),
		processed_at:   Some(chrono::Utc::now()),
		processed_by:   Some("default".to_string()),
	};
	payment_repo.save(pre_processed_payment).await.unwrap();

	// Push payment to queue (it should be skipped by the worker)
	redis_queue
		.push(Message::with(
			payment_to_process.correlation_id,
			payment_to_process.clone(),
		))
		.await
		.unwrap();

	let worker_handle = tokio::spawn(payment_processing_worker(
		redis_queue.clone(),
		payment_repo.clone(),
		process_payment_use_case.clone(),
		router.clone(),
	));

	// Give the worker some time to process
	tokio::time::sleep(Duration::from_secs(5)).await;

	let now = chrono::Utc::now();
	let one_day_ago = now.checked_sub_days(Days::new(1)).unwrap();
	let (processed_payments, processed_amount) = payment_repo
		.get_summary_by_group("default", one_day_ago.timestamp(), now.timestamp())
		.await
		.unwrap();

	assert_eq!(processed_payments, 1);
	assert_eq!(processed_amount, 500.0);

	// Abort the worker to clean up
	worker_handle.abort();
}

#[tokio::test]
async fn test_payment_processing_worker_redis_failure() {
	let redis_container = get_test_redis_client().await;
	let redis_client = redis_container.client.clone();
	let redis_container_instance = redis_container.container;
	let http_client = Client::new();
	let breaker =
		CircuitBreaker::<DefaultPolicy, PaymentProcessingError>::builder().build();
	let redis_queue = PaymentQueue::new(redis_client.clone());
	let payment_repo = RedisPaymentRepository::new(redis_client.clone());
	let process_payment_use_case = ProcessPaymentUseCase::new(
		payment_repo.clone(),
		http_client.clone(),
		breaker,
	);
	let router = InMemoryPaymentRouter::new();

	// Stop the redis container to simulate a connection failure
	let _ = redis_container_instance.stop().await;

	let worker_handle = tokio::spawn(payment_processing_worker(
		redis_queue,
		payment_repo,
		process_payment_use_case,
		router,
	));

	// Give the worker some time to run
	tokio::time::sleep(Duration::from_secs(6)).await;

	// The worker should not panic and should still be running
	assert!(!worker_handle.is_finished());

	// Abort the worker to clean up
	worker_handle.abort();
}
