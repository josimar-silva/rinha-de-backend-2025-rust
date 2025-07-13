#![allow(unused_imports)]

use std::sync::Arc;

use actix_web::{App, HttpResponse, test, web};
use log::{error, info};
use redis::AsyncCommands;
use reqwest::Client;
use rinha_de_backend::api::handlers::{
	PaymentRequest, PaymentsSummaryResponse, payments, payments_summary,
};
use rinha_de_backend::workers::payment_processors::{
	health_check_worker, payment_processing_worker,
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
		.del("health_default_failing")
		.await
		.expect("Failed to clear health_default_failing");
	let _: () = con
		.del("health_fallback_failing")
		.await
		.expect("Failed to clear health_fallback_failing");
	(client, container)
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
				HttpWaitStrategy::new("/").with_expected_status_code(200_u16),
			))
			.start()
			.await
			.unwrap();

	let fallback_processor_container =
		GenericImage::new("zanfranceschi/payment-processor", "latest")
			.with_exposed_port(testcontainers::core::ContainerPort::Tcp(8080))
			.with_wait_for(WaitFor::http(
				HttpWaitStrategy::new("/").with_expected_status_code(200_u16),
			))
			.start()
			.await
			.unwrap();

	let default_port = default_processor_container.get_host_port_ipv4(8080).await;
	let fallback_port = fallback_processor_container.get_host_port_ipv4(8080).await;

	let default_url = format!("http://127.0.0.1:{}", default_port.unwrap());
	let fallback_url = format!("http://127.0.0.1:{}", fallback_port.unwrap());

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
	tokio::time::sleep(Duration::from_secs(30)).await;

	let mut con = redis_client
		.get_multiplexed_async_connection()
		.await
		.unwrap();
	let default_failing: bool = con.get("health_default_failing").await.unwrap();
	let _default_min_response_time: u64 =
		con.get("health_default_min_response_time").await.unwrap();

	assert!(!default_failing);

	let fallback_failing: bool = con.get("health_fallback_failing").await.unwrap();
	let _fallback_min_response_time: u64 =
		con.get("health_fallback_min_response_time").await.unwrap();

	assert!(!fallback_failing);

	// Abort the worker to clean up
	worker_handle.abort();
}

#[actix_web::test]
#[ignore = "payment processors need proper setup"]
async fn test_payment_processing_worker_default_success() {
	let (redis_client, _redis_node) = get_test_redis_client().await;
	let (default_url, fallback_url, _default_node, _fallback_node) =
		setup_payment_processors().await;
	let http_client = Client::new();

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
	let _: () = con.set("health_default_failing", false).await.unwrap();
	let _: () = con.set("health_fallback_failing", true).await.unwrap(); // Fallback is failing

	let worker_handle = tokio::spawn(payment_processing_worker(
		redis_client.clone(),
		http_client,
		default_url.clone(),
		fallback_url.clone(),
	));

	// Give the worker some time to process the payment
	tokio::time::sleep(Duration::from_secs(30)).await;

	info!("Attempting to retrieve default total requests from Redis.");
	let default_total_requests: i64 = con
		.hget("payments_summary_default", "totalRequests")
		.await
		.unwrap();
	info!("Attempting to retrieve default total amount from Redis.");
	let default_total_amount: f64 = con
		.hget("payments_summary_default", "totalAmount")
		.await
		.unwrap();
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

	// Abort the worker to clean up
	worker_handle.abort();
}

#[actix_web::test]
#[ignore = "payment processors need proper setup"]
async fn test_payment_processing_worker_fallback_success() {
	let (redis_client, _redis_node) = get_test_redis_client().await;
	let (default_url, fallback_url, _default_node, _fallback_node) =
		setup_payment_processors().await;
	let http_client = Client::new();

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
	let _: () = con.set("health_default_failing", true).await.unwrap(); // Default is failing
	let _: () = con.set("health_fallback_failing", false).await.unwrap();

	let worker_handle = tokio::spawn(payment_processing_worker(
		redis_client.clone(),
		http_client,
		default_url.clone(),
		fallback_url.clone(),
	));

	// Give the worker some time to process the payment
	tokio::time::sleep(Duration::from_secs(20)).await;

	let fallback_total_requests: i64 = con
		.hget("payments_summary_fallback", "totalRequests")
		.await
		.unwrap();
	let fallback_total_amount: f64 = con
		.hget("payments_summary_fallback", "totalAmount")
		.await
		.unwrap();
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

	// Abort the worker to clean up
	worker_handle.abort();
}
