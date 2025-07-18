use reqwest::Client;
use rinha_de_backend::domain::health_status::HealthStatus;
use rinha_de_backend::domain::payment_processor::PaymentProcessor;
use rinha_de_backend::infrastructure::routing::in_memory_payment_router::InMemoryPaymentRouter;
use rinha_de_backend::infrastructure::workers::processor_health_monitor_worker::processor_health_monitor_worker;
use tokio::time::{Duration, sleep};

mod support;

use crate::support::payment_processor_container::setup_payment_processors;

#[tokio::test]
async fn test_processor_health_monitor_worker_healthy_processor() {
	let (default_processor_container, fallback_processor_container) =
		setup_payment_processors().await;
	let default_url = default_processor_container.url.clone();
	let fallback_url = fallback_processor_container.url.clone();
	let http_client = Client::new();
	let router = InMemoryPaymentRouter::new();

	// Spawn the worker
	let worker_handle = tokio::spawn(processor_health_monitor_worker(
		router.clone(),
		http_client.clone(),
		default_url.clone(),
		fallback_url.clone(),
	));

	// Give the worker some time to perform health checks
	sleep(Duration::from_secs(6)).await; // Worker sleeps for 5 seconds, so wait a bit more

	// Verify the default processor's health status in the router
	let processors = router.processors.read().unwrap();
	let default_processor = processors
		.get("default")
		.expect("Default processor not found");

	assert_eq!(default_processor.health, HealthStatus::Healthy);

	// Verify the fallback processor's health status in the router
	let fallback_processor = processors
		.get("fallback")
		.expect("Fallback processor not found");
	assert_eq!(fallback_processor.health, HealthStatus::Healthy);

	worker_handle.abort();
}

#[tokio::test]
async fn test_marks_processor_as_failing_when_unreachable() {
	let http_client = Client::new();
	let router = InMemoryPaymentRouter::new();

	// Initialize processors in the router
	router.update_processor_health(PaymentProcessor {
		name:              "default".to_string(),
		url:               "http://non-existent-default:8080".to_string(),
		health:            HealthStatus::Healthy, /* Initial state, will be
		                                           * updated to Failing */
		min_response_time: 0,
	});
	router.update_processor_health(PaymentProcessor {
		name:              "fallback".to_string(),
		url:               "http://non-existent-fallback:8080".to_string(),
		health:            HealthStatus::Healthy, /* Initial state, will be
		                                           * updated to Failing */
		min_response_time: 0,
	});

	// Use non-existent URLs to simulate unreachable processors
	let default_url = "http://non-existent-default:8080".to_string();
	let fallback_url = "http://non-existent-fallback:8080".to_string();

	let worker_handle = tokio::spawn(processor_health_monitor_worker(
		router.clone(),
		http_client.clone(),
		default_url.clone(),
		fallback_url.clone(),
	));

	// Give the worker some time to attempt health checks and fail
	sleep(Duration::from_secs(6)).await;

	let processors = router.processors.read().unwrap();

	let default_processor = processors
		.get("default")
		.expect("Default processor not found");
	assert_eq!(default_processor.health, HealthStatus::Failing);

	let fallback_processor = processors
		.get("fallback")
		.expect("Fallback processor not found");
	assert_eq!(fallback_processor.health, HealthStatus::Failing);

	worker_handle.abort();
}

#[tokio::test]
async fn test_should_not_panic_an_error_occurs() {
	let http_client = Client::new();
	let router = InMemoryPaymentRouter::new();

	// Initialize processors in the router
	router.update_processor_health(PaymentProcessor {
		name:              "default".to_string(),
		url:               "http://another-non-existent-default:8080".to_string(),
		health:            HealthStatus::Healthy, /* Initial state, will be
		                                           * updated to Failing */
		min_response_time: 0,
	});
	router.update_processor_health(PaymentProcessor {
		name:              "fallback".to_string(),
		url:               "http://another-non-existent-fallback:8080".to_string(),
		health:            HealthStatus::Healthy, /* Initial state, will be
		                                           * updated to Failing */
		min_response_time: 0,
	});

	// Use non-existent URLs to simulate unreachable processors
	let default_url = "http://another-non-existent-default:8080".to_string();
	let fallback_url = "http://another-non-existent-fallback:8080".to_string();

	let worker_handle = tokio::spawn(processor_health_monitor_worker(
		router.clone(),
		http_client.clone(),
		default_url.clone(),
		fallback_url.clone(),
	));

	// Give the worker some time to encounter errors
	sleep(Duration::from_secs(6)).await;

	// Assert that the worker is still running (hasn't panicked)
	assert!(!worker_handle.is_finished());

	worker_handle.abort();
}
