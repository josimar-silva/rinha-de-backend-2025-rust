use reqwest::Client;
use rinha_de_backend::domain::health_status::HealthStatus;
use rinha_de_backend::domain::payment_processor::PaymentProcessor;
use rinha_de_backend::infrastructure::routing::in_memory_payment_router::InMemoryPaymentRouter;
use rinha_de_backend::infrastructure::workers::processor_health_monitor_worker::processor_health_monitor_worker;
use tokio::time::{Duration, sleep};

mod support;

use crate::support::payment_processor_container::setup_payment_processors;

#[tokio::test]
async fn test_update_processor_health_when_processor_is_reachable() {
	let (default_processor_container, fallback_processor_container) =
		setup_payment_processors().await;
	let default_url = default_processor_container.url.clone();
	let fallback_url = fallback_processor_container.url.clone();
	let http_client = Client::builder()
		.timeout(Duration::from_secs(2))
		.build()
		.unwrap();
	let router = InMemoryPaymentRouter::new();

	// Spawn the worker
	let worker_handle = tokio::spawn(processor_health_monitor_worker(
		router.clone(),
		http_client.clone(),
		default_url.clone(),
		fallback_url.clone(),
	));

	wait_for_workflow_to_run().await;

	let processors = router.processors.read().unwrap();
	let default_processor = processors
		.get("default")
		.expect("Default processor not found");

	assert_eq!(default_processor.health, HealthStatus::Healthy);

	let fallback_processor = processors
		.get("fallback")
		.expect("Fallback processor not found");
	assert_eq!(fallback_processor.health, HealthStatus::Healthy);

	worker_handle.abort();
}

#[tokio::test]
async fn test_marks_processor_as_failing_when_unreachable() {
	let http_client = Client::builder()
		.timeout(Duration::from_secs(2))
		.build()
		.unwrap();
	let default_url = "http://non-existent-default:8080".to_string();
	let fallback_url = "http://non-existent-fallback:8080".to_string();
	let router = InMemoryPaymentRouter::new();

	router.update_processor_health(PaymentProcessor {
		name:              "default".to_string(),
		url:               default_url.clone(),
		health:            HealthStatus::Healthy,
		min_response_time: 0,
	});
	router.update_processor_health(PaymentProcessor {
		name:              "fallback".to_string(),
		url:               fallback_url.clone(),
		health:            HealthStatus::Healthy,
		min_response_time: 0,
	});

	let worker_handle = tokio::spawn(processor_health_monitor_worker(
		router.clone(),
		http_client.clone(),
		default_url.clone(),
		fallback_url.clone(),
	));

	wait_for_workflow_to_run().await;

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
	let http_client = Client::builder()
		.timeout(Duration::from_secs(2))
		.build()
		.unwrap();
	let router = InMemoryPaymentRouter::new();

	router.update_processor_health(PaymentProcessor {
		name:              "default".to_string(),
		url:               "http://another-non-existent-default:8080".to_string(),
		health:            HealthStatus::Healthy,
		min_response_time: 0,
	});
	router.update_processor_health(PaymentProcessor {
		name:              "fallback".to_string(),
		url:               "http://another-non-existent-fallback:8080".to_string(),
		health:            HealthStatus::Healthy,
		min_response_time: 0,
	});

	let default_non_existent_url =
		"http://another-non-existent-default:8080".to_string();
	let fallback_non_existent_url =
		"http://another-non-existent-fallback:8080".to_string();

	let worker_handle = tokio::spawn(processor_health_monitor_worker(
		router.clone(),
		http_client.clone(),
		default_non_existent_url.clone(),
		fallback_non_existent_url.clone(),
	));

	wait_for_workflow_to_run().await;

	assert!(!worker_handle.is_finished());

	worker_handle.abort();
}

async fn wait_for_workflow_to_run() {
	sleep(Duration::from_secs(6)).await;
}
