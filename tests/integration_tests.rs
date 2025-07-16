use actix_web::{App, test, web};
use log::info;
use redis::AsyncCommands;
use reqwest::Client;
use rinha_de_backend::api::handlers::{
	PaymentRequest, PaymentsSummaryResponse, payments, payments_summary,
};
use rinha_de_backend::workers::payment_processors::{
	health_check_worker, payment_processing_worker,
};
use tokio::time::Duration;
use uuid::Uuid;

mod support;

use crate::support::payment_processor_container::setup_payment_processors;
use crate::support::redis_container::get_test_redis_client;

#[actix_web::test]
async fn test_payments_post() {
	let (redis_client, redis_container) = get_test_redis_client().await;
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

	// Clean up
	redis_container.stop().await.unwrap();
}

#[actix_web::test]
async fn test_payments_summary_get_empty() {
	let (redis_client, redis_container) = get_test_redis_client().await;
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

	// Clean up
	redis_container.stop().await.unwrap();
}

#[actix_web::test]
async fn test_payments_summary_get_with_data() {
	let (redis_client, redis_container) = get_test_redis_client().await;
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

	// Clean up
	redis_container.stop().await.unwrap();
}

#[actix_web::test]
async fn test_health_check_worker_success() {
	let (redis_client, redis_container) = get_test_redis_client().await;
	let (default_url, fallback_url, default_container, fallback_container) =
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
	let default_failing: i32 = con.hget("health:default", "failing").await.unwrap();
	let _default_min_response_time: u64 = con
		.hget("health:default", "min_response_time")
		.await
		.unwrap();

	assert_eq!(default_failing, 0);

	let _fallback_min_response_time: u64 = con
		.hget("health:fallback", "min_response_time")
		.await
		.unwrap();

	// Abort the worker to clean up
	worker_handle.abort();

	// Stop all the containers
	redis_container.stop().await.unwrap();
	default_container.stop().await.unwrap();
	fallback_container.stop().await.unwrap();
}

#[actix_web::test]
async fn test_payment_processing_worker_default_success() {
	let (redis_client, redis_container) = get_test_redis_client().await;
	let (default_url, fallback_url, default_container, fallback_container) =
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
	let _: () = con.hset("health:default", "failing", 0).await.unwrap();
	let _: () = con.hset("health:fallback", "failing", 1).await.unwrap(); // Fallback is failing

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

	// Stop all the containers
	redis_container.stop().await.unwrap();
	default_container.stop().await.unwrap();
	fallback_container.stop().await.unwrap();
}

#[actix_web::test]
async fn test_payment_processing_worker_fallback_success() {
	let (redis_client, redis_container) = get_test_redis_client().await;
	let (default_url, fallback_url, default_container, fallback_container) =
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
	let _: () = con.hset("health:default", "failing", 1).await.unwrap(); // Default is failing
	let _: () = con.hset("health:fallback", "failing", 0).await.unwrap();

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

	// Stop all the containers
	redis_container.stop().await.unwrap();
	default_container.stop().await.unwrap();
	fallback_container.stop().await.unwrap();
}

#[actix_web::test]
#[ignore = "re-queue needs to be reviewed"]
async fn test_payment_processing_worker_requeue_on_failure() {
	let (redis_client, redis_container) = get_test_redis_client().await;
	let http_client = Client::new();

	let payment_req = PaymentRequest {
		correlation_id: Uuid::new_v4(),
		amount:         400.0,
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
	let _: () = con.hset("health:default", "failing", 1).await.unwrap(); // Both are failing
	let _: () = con.hset("health:fallback", "failing", 1).await.unwrap();

	let worker_handle = tokio::spawn(payment_processing_worker(
		redis_client.clone(),
		http_client,
		"http://non-existent-url:8080".to_string(),
		"http://non-existent-url:8080".to_string(),
	));

	// Give the worker some time to attempt processing and re-queue
	tokio::time::sleep(Duration::from_secs(5)).await;

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

	// Abort the worker to clean up
	worker_handle.abort();

	// Stop all the containers
	redis_container.stop().await.unwrap();
}

#[actix_web::test]
async fn test_payment_processing_worker_skip_processed_correlation_id() {
	let (redis_client, redis_container) = get_test_redis_client().await;
	let (default_url, fallback_url, default_container, fallback_container) =
		setup_payment_processors().await;
	let http_client = Client::new();

	let payment_req = PaymentRequest {
		correlation_id: Uuid::new_v4(),
		amount:         500.0,
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
		.sadd(
			"processed_correlation_ids",
			payment_req.correlation_id.to_string(),
		)
		.await
		.unwrap();
	let _: () = con.hset("health:default", "failing", 0).await.unwrap();
	let _: () = con.hset("health:fallback", "failing", 1).await.unwrap();

	let worker_handle = tokio::spawn(payment_processing_worker(
		redis_client.clone(),
		http_client,
		default_url.clone(),
		fallback_url.clone(),
	));

	// Give the worker some time to process
	tokio::time::sleep(Duration::from_secs(20)).await;

	let default_total_requests: i64 = con
		.hget("payments_summary_default", "totalRequests")
		.await
		.unwrap_or(0);

	assert_eq!(default_total_requests, 0);

	// Abort the worker to clean up
	worker_handle.abort();

	// Stop all the containers
	redis_container.stop().await.unwrap();
	default_container.stop().await.unwrap();
	fallback_container.stop().await.unwrap();
}

#[actix_web::test]
async fn test_payments_post_redis_failure() {
	let (redis_client, redis_node) = get_test_redis_client().await;
	let app = test::init_service(
		App::new()
			.app_data(web::Data::new(redis_client.clone()))
			.service(web::resource("/payments").route(web::post().to(payments))),
	)
	.await;

	// Stop the redis container to simulate a connection failure
	let _ = redis_node.stop().await;

	let payment_req = PaymentRequest {
		correlation_id: Uuid::new_v4(),
		amount:         100.0,
	};

	let req = test::TestRequest::post()
		.uri("/payments")
		.set_json(&payment_req)
		.to_request();
	let resp = test::call_service(&app, req).await;

	assert!(resp.status().is_server_error());
}

#[actix_web::test]
async fn test_payments_summary_get_redis_failure() {
	let (redis_client, redis_node) = get_test_redis_client().await;
	let app = test::init_service(
		App::new()
			.app_data(web::Data::new(redis_client.clone()))
			.service(
				web::resource("/payments-summary")
					.route(web::get().to(payments_summary)),
			),
	)
	.await;

	// Stop the redis container to simulate a connection failure
	let _ = redis_node.stop().await;

	let req = test::TestRequest::get()
		.uri("/payments-summary")
		.to_request();
	let resp = test::call_service(&app, req).await;

	assert!(resp.status().is_server_error());
}

#[actix_web::test]
async fn test_payment_processing_worker_redis_failure() {
	let (redis_client, redis_node) = get_test_redis_client().await;
	let http_client = Client::new();

	// Stop the redis container to simulate a connection failure
	let _ = redis_node.stop().await;

	let worker_handle = tokio::spawn(payment_processing_worker(
		redis_client.clone(),
		http_client,
		"http://localhost:8080".to_string(),
		"http://localhost:8080".to_string(),
	));

	// Give the worker some time to run
	tokio::time::sleep(Duration::from_secs(6)).await;

	// The worker should not panic and should still be running
	assert!(!worker_handle.is_finished());

	// Abort the worker to clean up
	worker_handle.abort();

	// Stop all the containers
	redis_node.stop().await.unwrap();
}

#[actix_web::test]
async fn test_health_check_worker_redis_failure() {
	let (redis_client, redis_node) = get_test_redis_client().await;
	let http_client = Client::new();

	// Stop the redis container to simulate a connection failure
	let _ = redis_node.stop().await;

	let worker_handle = tokio::spawn(health_check_worker(
		redis_client.clone(),
		http_client,
		"http://localhost:8080".to_string(),
		"http://localhost:8080".to_string(),
	));

	// Give the worker some time to run
	tokio::time::sleep(Duration::from_secs(6)).await;

	// The worker should not panic and should still be running
	assert!(!worker_handle.is_finished());

	// Abort the worker to clean up
	worker_handle.abort();

	// Stop all the containers
	redis_node.stop().await.unwrap();
}

#[actix_web::test]
async fn test_payment_processing_worker_deserialization_error() {
	let (redis_client, redis_container) = get_test_redis_client().await;
	let http_client = Client::new();

	let mut con = redis_client
		.get_multiplexed_async_connection()
		.await
		.unwrap();

	// Push a malformed payment to the queue
	let _: () = con
		.lpush("payments_queue", "not a valid json")
		.await
		.unwrap();

	let worker_handle = tokio::spawn(payment_processing_worker(
		redis_client.clone(),
		http_client,
		"http://localhost:8080".to_string(),
		"http://localhost:8080".to_string(),
	));

	// Give the worker some time to run
	tokio::time::sleep(Duration::from_secs(6)).await;

	// The worker should not panic and should still be running
	assert!(!worker_handle.is_finished());

	// Abort the worker to clean up
	worker_handle.abort();

	// Stop all the containers
	redis_container.stop().await.unwrap();
}

#[actix_web::test]
async fn test_run_bind_error() {
	let listener = std::net::TcpListener::bind("0.0.0.0:9999").unwrap();
	assert!(rinha_de_backend::run().await.is_err());
	drop(listener);
}

#[actix_web::test]
async fn test_health_check_worker_http_failure() {
	let (redis_client, redis_container) = get_test_redis_client().await;
	let http_client = Client::new();

	// Use a non-existent URL to simulate HTTP failure
	let worker_handle = tokio::spawn(health_check_worker(
		redis_client.clone(),
		http_client,
		"http://non-existent-url:8080".to_string(),
		"http://non-existent-url:8080".to_string(),
	));

	// Give the worker some time to attempt the HTTP call and update Redis
	tokio::time::sleep(Duration::from_secs(10)).await;

	let mut con = redis_client
		.get_multiplexed_async_connection()
		.await
		.unwrap();
	let default_failing: i32 =
		con.hget("health:default", "failing").await.unwrap_or(0);
	let fallback_failing: i32 =
		con.hget("health:fallback", "failing").await.unwrap_or(0);

	assert_eq!(default_failing, 1);
	assert_eq!(fallback_failing, 1);

	// Abort the worker to clean up
	worker_handle.abort();

	// Stop all the containers
	redis_container.stop().await.unwrap();
}
