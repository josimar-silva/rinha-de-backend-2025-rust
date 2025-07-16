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
	let (redis_client, _) = get_test_redis_client().await;
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

#[actix_web::test]
async fn test_payments_summary_get_redis_failure() {
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

	// Stop the redis container to simulate a connection failure
	let _ = redis_container.stop().await;

	let req = test::TestRequest::get()
		.uri("/payments-summary")
		.to_request();
	let resp = test::call_service(&app, req).await;

	assert!(resp.status().is_server_error());
}
