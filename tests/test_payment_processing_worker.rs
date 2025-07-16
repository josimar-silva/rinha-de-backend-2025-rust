use log::info;
use redis::AsyncCommands;
use reqwest::Client;
use rinha_de_backend::api::schema::PaymentRequest;
use rinha_de_backend::config::{
	DEFAULT_PAYMENT_SUMMARY_KEY, DEFAULT_PROCESSOR_HEALTH_KEY,
	FALLBACK_PAYMENT_SUMMARY_KEY, FALLBACK_PROCESSOR_HEALTH_KEY, PAYMENTS_QUEUE_KEY,
	PROCESSED_PAYMENTS_SET_KEY,
};
use rinha_de_backend::workers::payment_processor_worker::payment_processing_worker;
use tokio::time::Duration;
use uuid::Uuid;

mod support;

use crate::support::payment_processor_container::setup_payment_processors;
use crate::support::redis_container::get_test_redis_client;

#[tokio::test]
async fn test_payment_processing_worker_default_success() {
	let (redis_client, _) = get_test_redis_client().await;
	let (default_url, fallback_url, _, _) = setup_payment_processors().await;
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
			PAYMENTS_QUEUE_KEY,
			serde_json::to_string(&payment_req).unwrap(),
		)
		.await
		.unwrap();
	info!("Payment pushed to queue.");
	let _: () = con
		.hset(DEFAULT_PROCESSOR_HEALTH_KEY, "failing", 0)
		.await
		.unwrap();
	let _: () = con
		.hset(FALLBACK_PROCESSOR_HEALTH_KEY, "failing", 1)
		.await
		.unwrap(); // Fallback is failing

	let worker_handle = tokio::spawn(payment_processing_worker(
		redis_client.clone(),
		http_client,
		default_url.clone(),
		fallback_url.clone(),
	));

	// Give the worker some time to process the payment
	tokio::time::sleep(Duration::from_secs(30)).await;

	let processed_key = format!(
		"{}:{}",
		DEFAULT_PAYMENT_SUMMARY_KEY, payment_req.correlation_id
	);
	let processed_amount: f64 = con.hget(&processed_key, "amount").await.unwrap();
	let processed_at: i64 = con.hget(&processed_key, "processed_at").await.unwrap();

	let score: i64 = con
		.zscore(
			PROCESSED_PAYMENTS_SET_KEY,
			payment_req.correlation_id.to_string(),
		)
		.await
		.unwrap();

	assert_eq!(processed_amount, 250.0);
	assert_eq!(score, processed_at);

	// Abort the worker to clean up
	worker_handle.abort();
}

#[tokio::test]
async fn test_payment_processing_worker_fallback_success() {
	let (redis_client, _) = get_test_redis_client().await;
	let (default_url, fallback_url, _, _) = setup_payment_processors().await;
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
			PAYMENTS_QUEUE_KEY,
			serde_json::to_string(&payment_req).unwrap(),
		)
		.await
		.unwrap();
	info!("Payment pushed to queue.");
	let _: () = con
		.hset(DEFAULT_PROCESSOR_HEALTH_KEY, "failing", 1)
		.await
		.unwrap(); // Default is failing
	let _: () = con
		.hset(FALLBACK_PROCESSOR_HEALTH_KEY, "failing", 0)
		.await
		.unwrap();

	let worker_handle = tokio::spawn(payment_processing_worker(
		redis_client.clone(),
		http_client,
		default_url.clone(),
		fallback_url.clone(),
	));

	// Give the worker some time to process the payment
	tokio::time::sleep(Duration::from_secs(20)).await;

	let processed_key = format!(
		"{}:{}",
		FALLBACK_PAYMENT_SUMMARY_KEY, payment_req.correlation_id
	);
	let processed_amount: f64 = con.hget(&processed_key, "amount").await.unwrap();
	let processed_at: i64 = con.hget(&processed_key, "processed_at").await.unwrap();

	let score: i64 = con
		.zscore(
			PROCESSED_PAYMENTS_SET_KEY,
			payment_req.correlation_id.to_string(),
		)
		.await
		.unwrap();

	assert_eq!(processed_amount, 300.0);
	assert_eq!(score, processed_at);

	// Abort the worker to clean up
	worker_handle.abort();
}

#[tokio::test]
#[ignore = "re-queue needs to be reviewed"]
async fn test_payment_processing_worker_requeue_on_failure() {
	let (redis_client, _) = get_test_redis_client().await;
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
			PAYMENTS_QUEUE_KEY,
			serde_json::to_string(&payment_req).unwrap(),
		)
		.await
		.unwrap();
	info!("Payment pushed to queue.");
	let _: () = con
		.hset(DEFAULT_PROCESSOR_HEALTH_KEY, "failing", 1)
		.await
		.unwrap(); // Both are failing
	let _: () = con
		.hset(FALLBACK_PROCESSOR_HEALTH_KEY, "failing", 1)
		.await
		.unwrap();

	let worker_handle = tokio::spawn(payment_processing_worker(
		redis_client.clone(),
		http_client,
		"http://non-existent-url:8080".to_string(),
		"http://non-existent-url:8080".to_string(),
	));

	// Give the worker some time to attempt processing and re-queue
	tokio::time::sleep(Duration::from_secs(5)).await;

	let queued_payment: String = con
		.rpop::<&str, String>(PAYMENTS_QUEUE_KEY, None)
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
}

#[tokio::test]
async fn test_payment_processing_worker_skip_processed_correlation_id() {
	let (redis_client, _) = get_test_redis_client().await;
	let (default_url, fallback_url, _, _) = setup_payment_processors().await;
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
			PAYMENTS_QUEUE_KEY,
			serde_json::to_string(&payment_req).unwrap(),
		)
		.await
		.unwrap();
	info!("Payment pushed to queue.");
	let _: () = con
		.sadd(
			PROCESSED_PAYMENTS_SET_KEY,
			payment_req.correlation_id.to_string(),
		)
		.await
		.unwrap();
	let _: () = con
		.hset(DEFAULT_PROCESSOR_HEALTH_KEY, "failing", 0)
		.await
		.unwrap();
	let _: () = con
		.hset(FALLBACK_PROCESSOR_HEALTH_KEY, "failing", 1)
		.await
		.unwrap();

	let worker_handle = tokio::spawn(payment_processing_worker(
		redis_client.clone(),
		http_client,
		default_url.clone(),
		fallback_url.clone(),
	));

	// Give the worker some time to process
	tokio::time::sleep(Duration::from_secs(20)).await;

	let processed_key = format!(
		"{}:{}",
		DEFAULT_PAYMENT_SUMMARY_KEY, payment_req.correlation_id
	);
	let processed_amount: Option<f64> =
		con.hget(&processed_key, "amount").await.unwrap();

	assert!(processed_amount.is_none());

	// Abort the worker to clean up
	worker_handle.abort();
}

#[tokio::test]
async fn test_payment_processing_worker_redis_failure() {
	let (redis_client, redis_container) = get_test_redis_client().await;
	let http_client = Client::new();

	// Stop the redis container to simulate a connection failure
	let _ = redis_container.stop().await;

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
}

#[tokio::test]
async fn test_payment_processing_worker_deserialization_error() {
	let (redis_client, _) = get_test_redis_client().await;
	let http_client = Client::new();

	let mut con = redis_client
		.get_multiplexed_async_connection()
		.await
		.unwrap();

	// Push a malformed payment to the queue
	let _: () = con
		.lpush(PAYMENTS_QUEUE_KEY, "not a valid json")
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
}
