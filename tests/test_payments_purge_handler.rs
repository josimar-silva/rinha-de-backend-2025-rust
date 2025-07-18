use actix_web::{App, test, web};
use chrono::Utc;
use rinha_de_backend::adapters::web::handlers::payments_purge;
use rinha_de_backend::domain::repository::PaymentRepository;
use rinha_de_backend::infrastructure::persistence::redis_payment_repository::RedisPaymentRepository;
use rinha_de_backend::use_cases::purge_payments::PurgePaymentsUseCase;
use uuid::Uuid;

mod support;

use rinha_de_backend::domain::payment::Payment;

use crate::support::redis_container::get_test_redis_client;

#[actix_web::test]
async fn test_payments_purge_returns_success() {
	let redis_container = get_test_redis_client().await;
	let redis_client = redis_container.client.clone();
	let payment_repository = RedisPaymentRepository::new(redis_client.clone());
	let purge_payments_use_case =
		PurgePaymentsUseCase::new(payment_repository.clone());

	let app = test::init_service(
		App::new()
			.app_data(web::Data::new(purge_payments_use_case.clone()))
			.service(payments_purge),
	)
	.await;

	// Save some dummy payments
	let payment1 = Payment {
		correlation_id: Uuid::new_v4(),
		amount:         100.0,
		requested_at:   Some(Utc::now()),
		processed_at:   Some(Utc::now()),
		processed_by:   Some("group1".to_string()),
	};
	let payment2 = Payment {
		correlation_id: Uuid::new_v4(),
		amount:         200.0,
		requested_at:   Some(Utc::now()),
		processed_at:   Some(Utc::now()),
		processed_by:   Some("group2".to_string()),
	};
	payment_repository.save(payment1.clone()).await.unwrap();
	payment_repository.save(payment2.clone()).await.unwrap();

	// Verify payments are saved
	let is_processed1 = payment_repository
		.is_already_processed(&payment1.correlation_id.to_string())
		.await
		.unwrap();
	let is_processed2 = payment_repository
		.is_already_processed(&payment2.correlation_id.to_string())
		.await
		.unwrap();
	assert!(is_processed1);
	assert!(is_processed2);

	let req = test::TestRequest::post()
		.uri("/purge-payments")
		.to_request();
	let resp = test::call_service(&app, req).await;

	assert!(resp.status().is_success());

	// Verify payments are purged
	let is_processed1_after_purge = payment_repository
		.is_already_processed(&payment1.correlation_id.to_string())
		.await
		.unwrap();
	let is_processed2_after_purge = payment_repository
		.is_already_processed(&payment2.correlation_id.to_string())
		.await
		.unwrap();
	assert!(!is_processed1_after_purge);
	assert!(!is_processed2_after_purge);
}
