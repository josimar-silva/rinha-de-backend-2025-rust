use actix_web::{App, test, web};
use redis::AsyncCommands;
use rinha_de_backend::adapters::web::handlers::payments_summary;
use rinha_de_backend::infrastructure::persistence::redis_payment_repository::RedisPaymentRepository;
use rinha_de_backend::use_cases::dto::PaymentsSummaryResponse;
use rinha_de_backend::use_cases::get_payment_summary::GetPaymentSummaryUseCase;

mod support;

use crate::support::redis_container::get_test_redis_client;

#[actix_web::test]
async fn test_payments_summary_get_empty() {
	let redis_container = get_test_redis_client().await;
	let redis_client = redis_container.client.clone();
	let redis_repo = RedisPaymentRepository::new(redis_client.clone());
	let get_payment_summary_use_case =
		GetPaymentSummaryUseCase::new(redis_repo.clone());

	let app = test::init_service(
		App::new()
			.app_data(web::Data::new(get_payment_summary_use_case.clone()))
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
	let redis_container = get_test_redis_client().await;
	let redis_client = redis_container.client.clone();
	let mut con = redis_client
		.get_multiplexed_async_connection()
		.await
		.unwrap();

	let now = chrono::Utc::now().timestamp();

	let _: () = con
		.hset("payment_summary:default:d1", "amount", 1000.43)
		.await
		.unwrap();
	let _: () = con.zadd("processed_payments", "d1", now).await.unwrap();

	let _: () = con
		.hset("payment_summary:default:d2", "amount", 2000.16)
		.await
		.unwrap();
	let _: () = con.zadd("processed_payments", "d2", now).await.unwrap();

	let _: () = con
		.hset("payment_summary:fallback:f1", "amount", 500.42)
		.await
		.unwrap();
	let _: () = con.zadd("processed_payments", "f1", now).await.unwrap();

	let redis_repo = RedisPaymentRepository::new(redis_client.clone());
	let get_payment_summary_use_case =
		GetPaymentSummaryUseCase::new(redis_repo.clone());

	let app = test::init_service(
		App::new()
			.app_data(web::Data::new(get_payment_summary_use_case.clone()))
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
	assert_eq!(summary.default.total_amount, 3000.59);
	assert_eq!(summary.fallback.total_requests, 1);
	assert_eq!(summary.fallback.total_amount, 500.42);
}

#[actix_web::test]
async fn test_payments_summary_get_redis_failure() {
	let redis_container = get_test_redis_client().await;
	let redis_client = redis_container.client.clone();
	let redis_repo = RedisPaymentRepository::new(redis_client.clone());
	let get_payment_summary_use_case =
		GetPaymentSummaryUseCase::new(redis_repo.clone());

	let app = test::init_service(
		App::new()
			.app_data(web::Data::new(get_payment_summary_use_case.clone()))
			.service(payments_summary),
	)
	.await;

	// Stop the redis container to simulate a connection failure
	let _ = redis_container.container.stop().await;

	let req = test::TestRequest::get()
		.uri("/payments-summary")
		.to_request();
	let resp = test::call_service(&app, req).await;

	assert!(resp.status().is_server_error());
}

#[actix_web::test]
async fn test_payments_summary_get_with_filter() {
	let redis_container = get_test_redis_client().await;
	let redis_client = redis_container.client.clone();
	let mut con = redis_client
		.get_multiplexed_async_connection()
		.await
		.unwrap();

	let now = chrono::Utc::now().timestamp();

	let _: () = con
		.hset("payment_summary:default:1", "amount", 1000.23)
		.await
		.unwrap();
	let _: () = con.zadd("processed_payments", "1", now).await.unwrap();

	let _: () = con
		.hset("payment_summary:default:2", "amount", 1000.27)
		.await
		.unwrap();
	let _: () = con.zadd("processed_payments", "2", now - 10).await.unwrap();

	let redis_repo = RedisPaymentRepository::new(redis_client.clone());
	let get_payment_summary_use_case =
		GetPaymentSummaryUseCase::new(redis_repo.clone());

	let app = test::init_service(
		App::new()
			.app_data(web::Data::new(get_payment_summary_use_case.clone()))
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
	assert_eq!(summary.default.total_amount, 1000.23);
	assert_eq!(summary.fallback.total_requests, 0);
	assert_eq!(summary.fallback.total_amount, 0.0);
}
