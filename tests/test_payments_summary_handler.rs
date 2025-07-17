use actix_web::{App, test, web};
use redis::AsyncCommands;
use rinha_de_backend::api::handlers::payments_summary;
use rinha_de_backend::api::schema::PaymentsSummaryResponse;

mod support;

use crate::support::redis_container::get_test_redis_client;

#[actix_web::test]
async fn test_payments_summary_get_empty() {
	let (redis_client, _) = get_test_redis_client().await;
	let app = test::init_service(
		App::new()
			.app_data(web::Data::new(redis_client.clone()))
			.service(payments_summary),
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
	let (redis_client, _) = get_test_redis_client().await;
	let mut con = redis_client
		.get_multiplexed_async_connection()
		.await
		.unwrap();

	let now = chrono::Utc::now().timestamp();

	// Add two "default" payments
	let _: () = con
		.hset("payment_summary:default:d1", "amount", 1000.0)
		.await
		.unwrap();
	let _: () = con.zadd("processed_payments", "d1", now).await.unwrap();

	let _: () = con
		.hset("payment_summary:default:d2", "amount", 2000.0)
		.await
		.unwrap();
	let _: () = con.zadd("processed_payments", "d2", now).await.unwrap();

	// Add one "fallback" payment
	let _: () = con
		.hset("payment_summary:fallback:f1", "amount", 500.0)
		.await
		.unwrap();
	let _: () = con.zadd("processed_payments", "f1", now).await.unwrap();

	let app = test::init_service(
		App::new()
			.app_data(web::Data::new(redis_client.clone()))
			.service(payments_summary),
	)
	.await;

	let req = test::TestRequest::get()
		.uri("/payments-summary")
		.to_request();
	let resp = test::call_service(&app, req).await;

	assert!(resp.status().is_success());

	let summary: PaymentsSummaryResponse = test::read_body_json(resp).await;

	assert_eq!(summary.default.total_requests, 2);
	assert_eq!(summary.default.total_amount, 3000.0);
	assert_eq!(summary.fallback.total_requests, 1);
	assert_eq!(summary.fallback.total_amount, 500.0);
}

#[actix_web::test]
async fn test_payments_summary_get_redis_failure() {
	let (redis_client, redis_container) = get_test_redis_client().await;
	let app = test::init_service(
		App::new()
			.app_data(web::Data::new(redis_client.clone()))
			.service(payments_summary),
	)
	.await;

	// Stop the redis container to simulate a connection failure
	let _ = redis_container.stop().await;

	let req = test::TestRequest::get()
		.uri("/payments-summary")
		.to_request();
	let resp = test::call_service(&app, req).await;

	assert!(resp.status().is_server_error());
}

#[actix_web::test]
async fn test_payments_summary_get_with_filter() {
	let (redis_client, _) = get_test_redis_client().await;
	let mut con = redis_client
		.get_multiplexed_async_connection()
		.await
		.unwrap();

	let now = chrono::Utc::now().timestamp();

	let _: () = con
		.hset("payment_summary:default:1", "amount", 1000.0)
		.await
		.unwrap();
	let _: () = con.zadd("processed_payments", "1", now).await.unwrap();

	let _: () = con
		.hset("payment_summary:default:2", "amount", 1000.0)
		.await
		.unwrap();
	let _: () = con.zadd("processed_payments", "2", now - 10).await.unwrap();

	let app = test::init_service(
		App::new()
			.app_data(web::Data::new(redis_client.clone()))
			.service(payments_summary),
	)
	.await;

	let from = chrono::Utc::now().timestamp() - 5;
	let to = chrono::Utc::now().timestamp() + 5;

	let req = test::TestRequest::get()
		.uri(&format!("/payments-summary?from={from}&to={to}"))
		.to_request();
	let resp = test::call_service(&app, req).await;

	assert!(resp.status().is_success());

	let summary: PaymentsSummaryResponse = test::read_body_json(resp).await;

	assert_eq!(summary.default.total_requests, 1);
	assert_eq!(summary.default.total_amount, 1000.0);
	assert_eq!(summary.fallback.total_requests, 0);
	assert_eq!(summary.fallback.total_amount, 0.0);
}
