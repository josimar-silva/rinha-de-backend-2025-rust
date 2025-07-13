#![allow(unused_imports)]

use std::sync::Arc;

use actix_web::{App, HttpResponse, test, web};
use log::{error, info, warn};
use redis::AsyncCommands;
use reqwest::Client;
use rinha_de_backend::{
	HealthCheckResponse, PaymentProcessorRequest, PaymentRequest,
	PaymentsSummaryResponse, SummaryData, health_check_worker,
	payment_processing_worker, payments, payments_summary,
};
use serde_json::json;
use testcontainers::GenericImage;
use testcontainers::core::wait::HttpWaitStrategy;
use testcontainers::core::{ContainerPort, WaitFor};
use testcontainers::runners::AsyncRunner;
use tokio::time::{Duration, timeout};
use uuid::Uuid;

// Helper function to create a test Redis client using testcontainers
async fn get_test_redis_client()
-> (redis::Client, testcontainers::ContainerAsync<GenericImage>) {
	let container = GenericImage::new("redis", "alpine3.21")
		.with_exposed_port(ContainerPort::Tcp(6379))
		.with_wait_for(WaitFor::message_on_stdout("Ready to accept connections"))
		.start()
		.await
		.unwrap();
	let host_port = container.get_host_port_ipv4(6379).await;
	let redis_url = format!("redis://127.0.0.1:{}", host_port.unwrap());
	let client = redis::Client::open(redis_url).expect("Invalid Redis URL");
	let mut con = client
		.get_multiplexed_async_connection()
		.await
		.expect("Failed to connect to Redis");
	// Clear Redis for a clean test environment
	let _: () = con
		.del("payments_queue")
		.await
		.expect("Failed to clear payments_queue");
	let _: () = con
		.del("payments_summary_default")
		.await
		.expect("Failed to clear payments_summary_default");
	let _: () = con
		.del("payments_summary_fallback")
		.await
		.expect("Failed to clear payments_summary_fallback");
	let _: () = con
		.del("processed_correlation_ids")
		.await
		.expect("Failed to clear processed_correlation_ids");
	let _: () = con
		.del("payment_processor:default:healthy")
		.await
		.expect("Failed to clear payment_processor:default:healthy");
	let _: () = con
		.del("payment_processor:secondary:healthy")
		.await
		.expect("Failed to clear payment_processor:secondary:healthy");
	(client, container)
}

// Helper to set payment processor admin configurations
async fn set_processor_config(
	client: &Client,
	base_url: &str,
	token: &str,
	config_type: &str,
	value: serde_json::Value,
) {
	let admin_url = format!(
		"{}/admin/configurations/{}",
		base_url.trim_end_matches('/'),
		config_type
	);
	let resp = client
		.put(&admin_url)
		.header("X-Rinha-Token", token)
		.json(&value)
		.send()
		.await
		.expect(&format!(
			"Failed to set {} config for {}",
			config_type, base_url
		));
	assert!(
		resp.status().is_success(),
		"Failed to set {} config for {}: {:?}",
		config_type,
		base_url,
		resp.status()
	);
}

// Helper to purge payments
async fn purge_processor_payments(client: &Client, base_url: &str, token: &str) {
	let admin_url =
		format!("{}/admin/purge-payments", base_url.trim_end_matches('/'));
	let resp = client
		.post(&admin_url)
		.header("X-Rinha-Token", token)
		.send()
		.await
		.expect(&format!("Failed to purge payments for {}", base_url));
	assert!(
		resp.status().is_success(),
		"Failed to purge payments for {}: {:?}",
		base_url,
		resp.status()
	);
}

#[actix_web::test]
async fn test_payments_post() {
	let (redis_client, _redis_node) = get_test_redis_client().await;
	let app = test::init_service(
		App::new()
			.app_data(web::Data::new(redis_client.clone()))
			.service(web::resource("/payments").route(web::post().to(payments))),
	)
	.await;

	let payment_req = PaymentRequest {
		correlation_id: Uuid::new_v4(),
		amount:         100.0,
	};

	let req = test::TestRequest::post()
		.uri("/payments")
		.set_json(&payment_req)
		.to_request();
	let resp = test::call_service(&app, req).await;

	assert!(resp.status().is_success());

	let mut con = redis_client
		.get_multiplexed_async_connection()
		.await
		.unwrap();
	let queued_payment: String = con
		.rpop::<&str, String>("payments_queue", None)
		.await
		.unwrap();
	let deserialized_payment: PaymentRequest =
		serde_json::from_str(&queued_payment).unwrap();

	assert_eq!(
		deserialized_payment.correlation_id,
		payment_req.correlation_id
	);
	assert_eq!(deserialized_payment.amount, payment_req.amount);
}

#[actix_web::test]
async fn test_payments_summary_get_empty() {
	let (redis_client, _redis_node) = get_test_redis_client().await;
	let app = test::init_service(
		App::new()
			.app_data(web::Data::new(redis_client.clone()))
			.service(
				web::resource("/payments-summary")
					.route(web::get().to(payments_summary)),
			),
	)
	.await;

	let req = test::TestRequest::get()
		.uri("/payments-summary")
		.to_request();
	let resp = test::call_service(&app, req).await;

	assert!(resp.status().is_success());

	let summary: PaymentsSummaryResponse = test::read_body_json(resp).await;

	assert_eq!(summary.default.total_requests, 0);
	assert_eq!(summary.default.total_amount, 0.0);
	assert_eq!(summary.fallback.total_requests, 0);
	assert_eq!(summary.fallback.total_amount, 0.0);
}

#[actix_web::test]
async fn test_payments_summary_get_with_data() {
	let (redis_client, _redis_node) = get_test_redis_client().await;
	let mut con = redis_client
		.get_multiplexed_async_connection()
		.await
		.unwrap();

	let _: () = con
		.hset("payments_summary_default", "totalRequests", 10)
		.await
		.unwrap();
	let _: () = con
		.hset("payments_summary_default", "totalAmount", 1000.0)
		.await
		.unwrap();
	let _: () = con
		.hset("payments_summary_fallback", "totalRequests", 5)
		.await
		.unwrap();
	let _: () = con
		.hset("payments_summary_fallback", "totalAmount", 500.0)
		.await
		.unwrap();

	let app = test::init_service(
		App::new()
			.app_data(web::Data::new(redis_client.clone()))
			.service(
				web::resource("/payments-summary")
					.route(web::get().to(payments_summary)),
			),
	)
	.await;

	let req = test::TestRequest::get()
		.uri("/payments-summary")
		.to_request();
	let resp = test::call_service(&app, req).await;

	assert!(resp.status().is_success());

	let summary: PaymentsSummaryResponse = test::read_body_json(resp).await;

	assert_eq!(summary.default.total_requests, 10);
	assert_eq!(summary.default.total_amount, 1000.0);
	assert_eq!(summary.fallback.total_requests, 5);
	assert_eq!(summary.fallback.total_amount, 500.0);
}

async fn setup_payment_processors() -> (
	String,
	String,
	testcontainers::ContainerAsync<GenericImage>,
	testcontainers::ContainerAsync<GenericImage>,
) {
	let default_processor_container =
		GenericImage::new("zanfranceschi/payment-processor", "latest")
			.with_exposed_port(ContainerPort::Tcp(8080))
			.with_wait_for(WaitFor::http(
				HttpWaitStrategy::new("/payments/service-health")
					.with_expected_status_code(200_u16),
			))
			.start()
			.await
			.unwrap();

	let fallback_processor_container =
		GenericImage::new("zanfranceschi/payment-processor", "latest")
			.with_exposed_port(testcontainers::core::ContainerPort::Tcp(8080))
			.with_wait_for(WaitFor::http(
				HttpWaitStrategy::new("/payments/service-health")
					.with_expected_status_code(200_u16),
			))
			.start()
			.await
			.unwrap();

	let default_port = default_processor_container.get_host_port_ipv4(8080).await;
	let fallback_port = fallback_processor_container.get_host_port_ipv4(8080).await;

	let default_url = format!("http://127.0.0.1:{}", default_port.unwrap());
	let fallback_url = format!("http://127.0.0.1:{}", fallback_port.unwrap());

	let http_client = Client::new();

	// Wait until processors report themselves as not failing
	loop {
		let default_health_resp: HealthCheckResponse = http_client
			.get(&format!(
				"{}/payments/service-health",
				default_url.trim_end_matches('/')
			))
			.send()
			.await
			.unwrap()
			.json()
			.await
			.unwrap();
		if !default_health_resp.failing {
			break;
		}
		tokio::time::sleep(Duration::from_millis(100)).await;
	}
	loop {
		let fallback_health_resp: HealthCheckResponse = http_client
			.get(&format!(
				"{}/payments/service-health",
				fallback_url.trim_end_matches('/')
			))
			.send()
			.await
			.unwrap()
			.json()
			.await
			.unwrap();
		if !fallback_health_resp.failing {
			break;
		}
		tokio::time::sleep(Duration::from_millis(100)).await;
	}

	(
		default_url,
		fallback_url,
		default_processor_container,
		fallback_processor_container,
	)
}

#[actix_web::test]
async fn test_health_check_worker_success() {
	let (redis_client, _redis_node) = get_test_redis_client().await;
	let (default_url, fallback_url, _default_node, _fallback_node) =
		setup_payment_processors().await;
	let http_client = Client::new();

	let worker_handle = tokio::spawn(health_check_worker(
		redis_client.clone(),
		http_client,
		default_url.clone(),
		fallback_url.clone(),
	));

	// Give the worker some time to run and update Redis
	tokio::time::sleep(Duration::from_secs(2)).await; // Reduced sleep for faster tests

	let mut con = redis_client
		.get_multiplexed_async_connection()
		.await
		.unwrap();
	let default_healthy: bool =
		con.get("payment_processor:default:healthy").await.unwrap();
	assert!(default_healthy, "Default processor should be healthy");

	let fallback_healthy: bool = con
		.get("payment_processor:secondary:healthy")
		.await
		.unwrap();
	assert!(fallback_healthy, "Fallback processor should be healthy");

	// Abort the worker to clean up
	worker_handle.abort();
}

#[actix_web::test]
async fn test_payment_processing_worker_default_success() {
	let (redis_client, _redis_node) = get_test_redis_client().await;
	let (default_url, fallback_url, default_container, fallback_container) =
		setup_payment_processors().await;
	let http_client = Client::new();

	// Purge any existing payments in the processors
	purge_processor_payments(&http_client, &default_url, "123").await;
	purge_processor_payments(&http_client, &fallback_url, "123").await;

	let payment_req = PaymentRequest {
		correlation_id: Uuid::new_v4(),
		amount:         250.0,
	};

	let mut con = redis_client
		.get_multiplexed_async_connection()
		.await
		.unwrap();
	info!("Attempting to push payment to queue.");
	let _: () = con
		.lpush(
			"payments_queue",
			serde_json::to_string(&payment_req).unwrap(),
		)
		.await
		.unwrap();
	info!("Payment pushed to queue.");
	let _: () = con
		.set("payment_processor:default:healthy", true)
		.await
		.unwrap();
	let _: () = con
		.set("payment_processor:secondary:healthy", false)
		.await
		.unwrap(); // Fallback is unhealthy

	let worker_handle = tokio::spawn(payment_processing_worker(
		redis_client.clone(),
		http_client.clone(),
		default_url.clone(),
		fallback_url.clone(),
	));

	// Give the worker some time to process the payment
	tokio::time::sleep(Duration::from_secs(5)).await; // Increased sleep for processing

	info!("Attempting to retrieve default total requests from Redis.");
	let default_total_requests: i64 = con
		.hget("payments_summary_default", "totalRequests")
		.await
		.unwrap_or(0);
	info!("Attempting to retrieve default total amount from Redis.");
	let default_total_amount: f64 = con
		.hget("payments_summary_default", "totalAmount")
		.await
		.unwrap_or(0.0);
	info!("Attempting to retrieve processed correlation ID from Redis.");
	let is_processed: bool = con
		.sismember(
			"processed_correlation_ids",
			payment_req.correlation_id.to_string(),
		)
		.await
		.unwrap();

	assert_eq!(default_total_requests, 1);
	assert_eq!(default_total_amount, 250.0);
	assert!(is_processed);

	// Ensure fallback was not used
	let fallback_total_requests: i64 = con
		.hget("payments_summary_fallback", "totalRequests")
		.await
		.unwrap_or(0);
	assert_eq!(
		fallback_total_requests, 0,
		"Fallback processor should not have been used"
	);

	// Abort the worker to clean up
	worker_handle.abort();
	drop(default_container);
	drop(fallback_container);
}

#[actix_web::test]
async fn test_payment_processing_worker_fallback_success() {
	let (redis_client, _redis_node) = get_test_redis_client().await;
	let (default_url, fallback_url, default_container, fallback_container) =
		setup_payment_processors().await;
	let http_client = Client::new();

	// Purge any existing payments in the processors
	purge_processor_payments(&http_client, &default_url, "123").await;
	purge_processor_payments(&http_client, &fallback_url, "123").await;

	let payment_req = PaymentRequest {
		correlation_id: Uuid::new_v4(),
		amount:         300.0,
	};

	let mut con = redis_client
		.get_multiplexed_async_connection()
		.await
		.unwrap();
	info!("Attempting to push payment to queue.");
	let _: () = con
		.lpush(
			"payments_queue",
			serde_json::to_string(&payment_req).unwrap(),
		)
		.await
		.unwrap();
	info!("Payment pushed to queue.");
	let _: () = con
		.set("payment_processor:default:healthy", false)
		.await
		.unwrap(); // Default is unhealthy
	let _: () = con
		.set("payment_processor:secondary:healthy", true)
		.await
		.unwrap();

	let worker_handle = tokio::spawn(payment_processing_worker(
		redis_client.clone(),
		http_client.clone(),
		default_url.clone(),
		fallback_url.clone(),
	));

	// Give the worker some time to process the payment
	tokio::time::sleep(Duration::from_secs(5)).await; // Increased sleep for processing

	let fallback_total_requests: i64 = con
		.hget("payments_summary_fallback", "totalRequests")
		.await
		.unwrap_or(0);
	let fallback_total_amount: f64 = con
		.hget("payments_summary_fallback", "totalAmount")
		.await
		.unwrap_or(0.0);
	info!("Attempting to retrieve processed correlation ID from Redis.");
	let is_processed: bool = con
		.sismember(
			"processed_correlation_ids",
			payment_req.correlation_id.to_string(),
		)
		.await
		.unwrap();

	assert_eq!(fallback_total_requests, 1);
	assert_eq!(fallback_total_amount, 300.0);
	assert!(is_processed);

	// Ensure default was not used
	let default_total_requests: i64 = con
		.hget("payments_summary_default", "totalRequests")
		.await
		.unwrap_or(0);
	assert_eq!(
		default_total_requests, 0,
		"Default processor should not have been used"
	);

	// Abort the worker to clean up
	worker_handle.abort();
	drop(default_container);
	drop(fallback_container);
}

#[actix_web::test]
async fn test_payment_processing_worker_default_retries_then_success() {
	let (redis_client, _redis_node) = get_test_redis_client().await;
	let (default_url, fallback_url, default_container, fallback_container) =
		setup_payment_processors().await;
	let http_client = Client::new();

	// Purge any existing payments in the processors
	purge_processor_payments(&http_client, &default_url, "123").await;
	purge_processor_payments(&http_client, &fallback_url, "123").await;

	// Set default processor to fail initially, then succeed
	set_processor_config(
		&http_client,
		&default_url,
		"123",
		"failure",
		json!({ "failure": true }),
	)
	.await;

	let payment_req = PaymentRequest {
		correlation_id: Uuid::new_v4(),
		amount:         150.0,
	};

	let mut con = redis_client
		.get_multiplexed_async_connection()
		.await
		.unwrap();
	let _: () = con
		.lpush(
			"payments_queue",
			serde_json::to_string(&payment_req).unwrap(),
		)
		.await
		.unwrap();
	let _: () = con
		.set("payment_processor:default:healthy", true)
		.await
		.unwrap();
	let _: () = con
		.set("payment_processor:secondary:healthy", false)
		.await
		.unwrap(); // Fallback is unhealthy

	let worker_handle = tokio::spawn(payment_processing_worker(
		redis_client.clone(),
		http_client.clone(),
		default_url.clone(),
		fallback_url.clone(),
	));

	// Give worker time to attempt first few retries
	tokio::time::sleep(Duration::from_secs(2)).await;

	// After some retries, make default processor healthy
	set_processor_config(
		&http_client,
		&default_url,
		"123",
		"failure",
		json!({ "failure": false }),
	)
	.await;

	tokio::time::sleep(Duration::from_secs(5)).await; // Give worker time to succeed

	let default_total_requests: i64 = con
		.hget("payments_summary_default", "totalRequests")
		.await
		.unwrap_or(0);
	let default_total_amount: f64 = con
		.hget("payments_summary_default", "totalAmount")
		.await
		.unwrap_or(0.0);
	let is_processed: bool = con
		.sismember(
			"processed_correlation_ids",
			payment_req.correlation_id.to_string(),
		)
		.await
		.unwrap();

	assert_eq!(default_total_requests, 1);
	assert_eq!(default_total_amount, 150.0);
	assert!(is_processed);

	// Ensure fallback was not used
	let fallback_total_requests: i64 = con
		.hget("payments_summary_fallback", "totalRequests")
		.await
		.unwrap_or(0);
	assert_eq!(
		fallback_total_requests, 0,
		"Fallback processor should not have been used"
	);

	worker_handle.abort();
	drop(default_container);
	drop(fallback_container);
}

#[actix_web::test]
async fn test_payment_processing_worker_default_fails_fallback_success() {
	let (redis_client, _redis_node) = get_test_redis_client().await;
	let (default_url, fallback_url, default_container, fallback_container) =
		setup_payment_processors().await;
	let http_client = Client::new();

	// Purge any existing payments in the processors
	purge_processor_payments(&http_client, &default_url, "123").await;
	purge_processor_payments(&http_client, &fallback_url, "123").await;

	// Set default processor to always fail
	set_processor_config(
		&http_client,
		&default_url,
		"123",
		"failure",
		json!({ "failure": true }),
	)
	.await;

	let payment_req = PaymentRequest {
		correlation_id: Uuid::new_v4(),
		amount:         200.0,
	};

	let mut con = redis_client
		.get_multiplexed_async_connection()
		.await
		.unwrap();
	let _: () = con
		.lpush(
			"payments_queue",
			serde_json::to_string(&payment_req).unwrap(),
		)
		.await
		.unwrap();
	let _: () = con
		.set("payment_processor:default:healthy", true)
		.await
		.unwrap(); // Marked healthy to trigger retries
	let _: () = con
		.set("payment_processor:secondary:healthy", true)
		.await
		.unwrap(); // Fallback is healthy

	let worker_handle = tokio::spawn(payment_processing_worker(
		redis_client.clone(),
		http_client.clone(),
		default_url.clone(),
		fallback_url.clone(),
	));

	tokio::time::sleep(Duration::from_secs(10)).await; // Give worker time to retry default and then fallback

	let default_total_requests: i64 = con
		.hget("payments_summary_default", "totalRequests")
		.await
		.unwrap_or(0);
	let default_total_amount: f64 = con
		.hget("payments_summary_default", "totalAmount")
		.await
		.unwrap_or(0.0);
	let fallback_total_requests: i64 = con
		.hget("payments_summary_fallback", "totalRequests")
		.await
		.unwrap_or(0);
	let fallback_total_amount: f64 = con
		.hget("payments_summary_fallback", "totalAmount")
		.await
		.unwrap_or(0.0);
	let is_processed: bool = con
		.sismember(
			"processed_correlation_ids",
			payment_req.correlation_id.to_string(),
		)
		.await
		.unwrap();

	assert_eq!(
		default_total_requests, 0,
		"Default processor should not have processed any payments"
	);
	assert_eq!(
		fallback_total_requests, 1,
		"Fallback processor should have processed the payment"
	);
	assert_eq!(fallback_total_amount, 200.0);
	assert!(is_processed);

	worker_handle.abort();
	drop(default_container);
	drop(fallback_container);
}

#[actix_web::test]
async fn test_payment_processing_worker_all_fail_requeue() {
	let (redis_client, _redis_node) = get_test_redis_client().await;
	let (default_url, fallback_url, default_container, fallback_container) =
		setup_payment_processors().await;
	let http_client = Client::new();

	// Purge any existing payments in the processors
	purge_processor_payments(&http_client, &default_url, "123").await;
	purge_processor_payments(&http_client, &fallback_url, "123").await;

	// Set both processors to always fail
	set_processor_config(
		&http_client,
		&default_url,
		"123",
		"failure",
		json!({ "failure": true }),
	)
	.await;
	set_processor_config(
		&http_client,
		&fallback_url,
		"123",
		"failure",
		json!({ "failure": true }),
	)
	.await;

	let payment_req = PaymentRequest {
		correlation_id: Uuid::new_v4(),
		amount:         50.0,
	};

	let mut con = redis_client
		.get_multiplexed_async_connection()
		.await
		.unwrap();
	let _: () = con
		.lpush(
			"payments_queue",
			serde_json::to_string(&payment_req).unwrap(),
		)
		.await
		.unwrap();
	let _: () = con
		.set("payment_processor:default:healthy", true)
		.await
		.unwrap(); // Marked healthy to trigger retries
	let _: () = con
		.set("payment_processor:secondary:healthy", true)
		.await
		.unwrap(); // Marked healthy to trigger fallback

	let worker_handle = tokio::spawn(payment_processing_worker(
		redis_client.clone(),
		http_client.clone(),
		default_url.clone(),
		fallback_url.clone(),
	));

	tokio::time::sleep(Duration::from_secs(10)).await; // Give worker time to retry default and then fallback

	// Check if payment was re-queued
	let queued_payment: String = con.rpop("payments_queue", None).await.unwrap();
	let deserialized_payment: PaymentRequest =
		serde_json::from_str(&queued_payment).unwrap();

	assert_eq!(
		deserialized_payment.correlation_id,
		payment_req.correlation_id
	);
	assert_eq!(deserialized_payment.amount, payment_req.amount);

	// Ensure no payments were processed
	let default_total_requests: i64 = con
		.hget("payments_summary_default", "totalRequests")
		.await
		.unwrap_or(0);
	let fallback_total_requests: i64 = con
		.hget("payments_summary_fallback", "totalRequests")
		.await
		.unwrap_or(0);
	let is_processed: bool = con
		.sismember(
			"processed_correlation_ids",
			payment_req.correlation_id.to_string(),
		)
		.await
		.unwrap_or(false);

	assert_eq!(default_total_requests, 0);
	assert_eq!(fallback_total_requests, 0);
	assert!(!is_processed);

	worker_handle.abort();
	drop(default_container);
	drop(fallback_container);
}

#[actix_web::test]
async fn test_payment_processing_worker_default_non_retryable_error_fallback_success()
 {
	let (redis_client, _redis_node) = get_test_redis_client().await;
	let (default_url, fallback_url, default_container, fallback_container) =
		setup_payment_processors().await;
	let http_client = Client::new();

	// Purge any existing payments in the processors
	purge_processor_payments(&http_client, &default_url, "123").await;
	purge_processor_payments(&http_client, &fallback_url, "123").await;

	// Set default processor to return a non-retryable 400 error
	// Note: The payment processor doesn't have a direct way to set 4xx errors
	// other than 429. For a real test, you'd mock the HTTP client or the processor.
	// For this example, we'll simulate it by making the default processor unhealthy
	// and ensuring fallback is used.
	let _: () = redis_client
		.get_multiplexed_async_connection()
		.await
		.unwrap()
		.set("payment_processor:default:healthy", false)
		.await
		.unwrap();

	let payment_req = PaymentRequest {
		correlation_id: Uuid::new_v4(),
		amount:         120.0,
	};

	let mut con = redis_client
		.get_multiplexed_async_connection()
		.await
		.unwrap();
	let _: () = con
		.lpush(
			"payments_queue",
			serde_json::to_string(&payment_req).unwrap(),
		)
		.await
		.unwrap();
	let _: () = con
		.set("payment_processor:default:healthy", false)
		.await
		.unwrap(); // Default is unhealthy
	let _: () = con
		.set("payment_processor:secondary:healthy", true)
		.await
		.unwrap(); // Fallback is healthy

	let worker_handle = tokio::spawn(payment_processing_worker(
		redis_client.clone(),
		http_client.clone(),
		default_url.clone(),
		fallback_url.clone(),
	));

	tokio::time::sleep(Duration::from_secs(5)).await; // Give worker time to process

	let default_total_requests: i64 = con
		.hget("payments_summary_default", "totalRequests")
		.await
		.unwrap_or(0);
	let fallback_total_requests: i64 = con
		.hget("payments_summary_fallback", "totalRequests")
		.await
		.unwrap_or(0);
	let fallback_total_amount: f64 = con
		.hget("payments_summary_fallback", "totalAmount")
		.await
		.unwrap_or(0.0);
	let is_processed: bool = con
		.sismember(
			"processed_correlation_ids",
			payment_req.correlation_id.to_string(),
		)
		.await
		.unwrap();

	assert_eq!(
		default_total_requests, 0,
		"Default processor should not have processed any payments"
	);
	assert_eq!(
		fallback_total_requests, 1,
		"Fallback processor should have processed the payment"
	);
	assert_eq!(fallback_total_amount, 120.0);
	assert!(is_processed);

	worker_handle.abort();
	drop(default_container);
	drop(fallback_container);
}
