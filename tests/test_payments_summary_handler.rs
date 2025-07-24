use std::ops::{Add, Sub};
use std::sync::Arc;
use std::time::Duration;

use actix_web::{App, test, web};
use futures::future::join_all;
use rinha_de_backend::adapters::web::handlers::payments_summary;
use rinha_de_backend::domain::payment::Payment;
use rinha_de_backend::domain::repository::PaymentRepository;
use rinha_de_backend::infrastructure::persistence::redis_payment_repository::RedisPaymentRepository;
use rinha_de_backend::use_cases::dto::PaymentsSummaryResponse;
use rinha_de_backend::use_cases::get_payment_summary::GetPaymentSummaryUseCase;
use time::OffsetDateTime;
use tokio::time::timeout;
use uuid::Uuid;

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
async fn test_get_payments_summary_without_filter_returns_all_data() {
	let redis_container = get_test_redis_client().await;
	let redis_client = redis_container.client.clone();
	let payment_repo = RedisPaymentRepository::new(redis_client.clone());

	let now = OffsetDateTime::now_utc();

	payment_repo
		.save(Payment {
			correlation_id: Uuid::new_v4(),
			amount:         1000.43,
			requested_at:   Some(now),
			processed_at:   Some(now),
			processed_by:   Some("default".to_string()),
		})
		.await
		.unwrap();

	payment_repo
		.save(Payment {
			correlation_id: Uuid::new_v4(),
			amount:         2000.16,
			requested_at:   Some(now),
			processed_at:   Some(now),
			processed_by:   Some("default".to_string()),
		})
		.await
		.unwrap();

	payment_repo
		.save(Payment {
			correlation_id: Uuid::new_v4(),
			amount:         500.42,
			requested_at:   Some(now),
			processed_at:   Some(now),
			processed_by:   Some("fallback".to_string()),
		})
		.await
		.unwrap();

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
async fn test_payments_summary_get_with_filter_simple_iso_8601() {
	let redis_container = get_test_redis_client().await;
	let redis_client = redis_container.client.clone();
	let payment_repo = RedisPaymentRepository::new(redis_client.clone());
	let get_payment_summary_use_case =
		GetPaymentSummaryUseCase::new(payment_repo.clone());

	let now = OffsetDateTime::now_utc();

	payment_repo
		.save(Payment {
			correlation_id: Uuid::new_v4(),
			amount:         1000.43,
			requested_at:   Some(now),
			processed_at:   Some(now),
			processed_by:   Some("default".to_string()),
		})
		.await
		.unwrap();

	let one_hour_ago = now.sub(time::Duration::hours(1));

	payment_repo
		.save(Payment {
			correlation_id: Uuid::new_v4(),
			amount:         2000.16,
			requested_at:   Some(one_hour_ago),
			processed_at:   Some(one_hour_ago),
			processed_by:   Some("default".to_string()),
		})
		.await
		.unwrap();

	payment_repo
		.save(Payment {
			correlation_id: Uuid::new_v4(),
			amount:         500.42,
			requested_at:   Some(now),
			processed_at:   Some(now),
			processed_by:   Some("fallback".to_string()),
		})
		.await
		.unwrap();

	let app = test::init_service(
		App::new()
			.app_data(web::Data::new(get_payment_summary_use_case.clone()))
			.service(payments_summary),
	)
	.await;

	let from = now
		.format(&time::format_description::well_known::Rfc3339)
		.unwrap();
	let to = now
		.add(time::Duration::hours(1))
		.format(&time::format_description::well_known::Rfc3339)
		.unwrap();

	let req = test::TestRequest::get()
		.uri(&format!("/payments-summary?from={from}&to={to}"))
		.to_request();
	let resp = test::call_service(&app, req).await;

	assert!(resp.status().is_success());

	let summary: PaymentsSummaryResponse = test::read_body_json(resp).await;

	assert_eq!(summary.default.total_requests, 1);
	assert_eq!(summary.default.total_amount, 1000.43);
	assert_eq!(summary.fallback.total_requests, 1);
	assert_eq!(summary.fallback.total_amount, 500.42);
}

#[actix_web::test]
async fn test_payments_summary_get_with_extended_iso_8601() {
	let redis_container = get_test_redis_client().await;
	let redis_client = redis_container.client.clone();
	let payment_repo = RedisPaymentRepository::new(redis_client.clone());

	let now = OffsetDateTime::now_utc();

	payment_repo
		.save(Payment {
			correlation_id: Uuid::new_v4(),
			amount:         1000.23,
			requested_at:   Some(now),
			processed_at:   Some(now),
			processed_by:   Some("default".to_string()),
		})
		.await
		.unwrap();

	let ten_hours_ago = now.sub(time::Duration::hours(10));

	payment_repo
		.save(Payment {
			correlation_id: Uuid::new_v4(),
			amount:         1000.27,
			requested_at:   Some(ten_hours_ago),
			processed_at:   Some(ten_hours_ago),
			processed_by:   Some("default".to_string()),
		})
		.await
		.unwrap();

	let redis_repo = RedisPaymentRepository::new(redis_client.clone());
	let get_payment_summary_use_case =
		GetPaymentSummaryUseCase::new(redis_repo.clone());

	let app = test::init_service(
		App::new()
			.app_data(web::Data::new(get_payment_summary_use_case.clone()))
			.service(payments_summary),
	)
	.await;

	let from = now
		.format(&time::format_description::well_known::Rfc3339)
		.unwrap();
	let to = now
		.add(time::Duration::hours(1))
		.format(&time::format_description::well_known::Rfc3339)
		.unwrap();

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

#[actix_web::test]
async fn test_redis_repository_concurrent_access() {
	let redis_container = get_test_redis_client().await;
	let redis_client = redis_container.client.clone();
	let payment_repo = Arc::new(RedisPaymentRepository::new(redis_client.clone()));

	const NUM_CONCURRENT_TASKS: usize = 50;
	const NUM_ITERATIONS_PER_TASK: usize = 100;

	let mut tasks = Vec::new();

	for _ in 0..NUM_CONCURRENT_TASKS {
		let repo = Arc::clone(&payment_repo);
		tasks.push(tokio::spawn(async move {
			for i in 0..NUM_ITERATIONS_PER_TASK {
				let from =
					OffsetDateTime::now_utc().sub(time::Duration::days(i as i64));
				let to =
					OffsetDateTime::now_utc().add(time::Duration::days(i as i64));

				// Call get_summary_by_group
				let result_summary =
					repo.get_summary_by_group("default", from, to).await;
				assert!(
					result_summary.is_ok(),
					"get_summary_by_group failed: {:?}",
					result_summary.err()
				);

				// Call is_already_processed
				let payment_id = Uuid::new_v4().to_string();
				let result_processed = repo.is_already_processed(&payment_id).await;
				assert!(
					result_processed.is_ok(),
					"is_already_processed failed: {:?}",
					result_processed.err()
				);
			}
		}));
	}

	let results = timeout(
		Duration::from_secs(60), /* Increased timeout for potentially blocking
		                          * operations */
		join_all(tasks),
	)
	.await;

	assert!(
		results.is_ok(),
		"Concurrent access test timed out or failed: {:?}",
		results.err()
	);
	for res in results.unwrap() {
		assert!(res.is_ok(), "A spawned task panicked: {:?}", res.err());
	}
}
#[actix_web::test]
async fn test_payments_summary_decimal_precision() {
	let redis_container = get_test_redis_client().await;
	let redis_client = redis_container.client.clone();
	let payment_repo = RedisPaymentRepository::new(redis_client.clone());

	let now = OffsetDateTime::now_utc();

	// Save payments with amounts having more than two decimal places
	payment_repo
		.save(Payment {
			correlation_id: Uuid::new_v4(),
			amount:         1000.12345,
			requested_at:   Some(now),
			processed_at:   Some(now),
			processed_by:   Some("default".to_string()),
		})
		.await
		.unwrap();

	payment_repo
		.save(Payment {
			correlation_id: Uuid::new_v4(),
			amount:         2000.6789,
			requested_at:   Some(now),
			processed_at:   Some(now),
			processed_by:   Some("default".to_string()),
		})
		.await
		.unwrap();

	payment_repo
		.save(Payment {
			correlation_id: Uuid::new_v4(),
			amount:         500.999,
			requested_at:   Some(now),
			processed_at:   Some(now),
			processed_by:   Some("fallback".to_string()),
		})
		.await
		.unwrap();

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
	assert_eq!(summary.default.total_amount, 3000.80); // 1000.12 + 2000.68
	assert_eq!(summary.fallback.total_requests, 1);
	assert_eq!(summary.fallback.total_amount, 501.00); // 500.999 rounds to 501.00
}
