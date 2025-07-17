use actix_web::{App, test, web};
use rinha_de_backend::adapters::web::handlers::payments;
use rinha_de_backend::adapters::web::schema::PaymentRequest;
use rinha_de_backend::domain::payment::Payment;
use rinha_de_backend::domain::queue::Queue;
use rinha_de_backend::infrastructure::queue::redis_payment_queue::PaymentQueue;
use rinha_de_backend::use_cases::create_payment::CreatePaymentUseCase;
use uuid::Uuid;

mod support;

use crate::support::redis_container::get_test_redis_client;

#[actix_web::test]
async fn test_payments_post_returns_success() {
	let redis_container = get_test_redis_client().await;
	let redis_client = redis_container.client.clone();
	let payment_queue = PaymentQueue::new(redis_client.clone());
	let create_payment_use_case = CreatePaymentUseCase::new(payment_queue.clone());

	let app = test::init_service(
		App::new()
			.app_data(web::Data::new(create_payment_use_case.clone()))
			.service(payments),
	)
	.await;

	let payment_req = PaymentRequest {
		correlation_id: Uuid::new_v4(),
		amount:         100.51,
	};

	let req = test::TestRequest::post()
		.uri("/payments")
		.set_json(&payment_req)
		.to_request();
	let resp = test::call_service(&app, req).await;

	assert!(resp.status().is_success());

	let message = payment_queue.pop().await.unwrap().unwrap();
	let deserialized_payment: Payment = message.body;

	assert_eq!(
		deserialized_payment.correlation_id,
		payment_req.correlation_id
	);
	assert_eq!(deserialized_payment.amount, payment_req.amount);
}

#[actix_web::test]
async fn test_payments_post_redis_failure() {
	let redis_container = get_test_redis_client().await;
	let redis_client = redis_container.client.clone();
	let payment_queue = PaymentQueue::new(redis_client.clone());
	let create_payment_use_case = CreatePaymentUseCase::new(payment_queue.clone());

	let app = test::init_service(
		App::new()
			.app_data(web::Data::new(create_payment_use_case.clone()))
			.service(payments),
	)
	.await;

	// Stop the redis container to simulate a connection failure
	let _ = redis_container.container.stop().await;

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
