use std::time::Duration;

use actix_web::{App, HttpServer, web};
use log::info;
use reqwest::Client;

pub mod api;
pub mod workers;

use crate::api::handlers::{payments, payments_summary};
use crate::workers::payment_processors::{
	health_check_worker, payment_processing_worker,
};

pub async fn run() -> std::io::Result<()> {
	env_logger::init();

	let redis_url = std::env::var("REDIS_URL")
		.unwrap_or_else(|_| "redis://127.0.0.1/".to_string());
	let redis_client = redis::Client::open(redis_url).expect("Invalid Redis URL");

	let default_processor_url = std::env::var("PAYMENT_PROCESSOR_URL_DEFAULT")
		.unwrap_or_else(|_| "http://payment-processor-1/".to_string());
	let fallback_processor_url = std::env::var("PAYMENT_PROCESSOR_URL_FALLBACK")
		.unwrap_or_else(|_| "http://payment-processor-2/".to_string());

	let http_client = Client::new();

	info!("Starting health check worker...");
	tokio::spawn(health_check_worker(
		redis_client.clone(),
		http_client.clone(),
		default_processor_url.clone(),
		fallback_processor_url.clone(),
	));

	info!("Starting payment processing worker...");
	tokio::spawn(payment_processing_worker(
		redis_client.clone(),
		http_client.clone(),
		default_processor_url.clone(),
		fallback_processor_url.clone(),
	));

	info!("Starting Actix-Web server on 0.0.0.0:9999...");
	HttpServer::new(move || {
		App::new()
			.app_data(web::Data::new(redis_client.clone()))
			.service(web::resource("/payments").route(web::post().to(payments)))
			.service(
				web::resource("/payments-summary")
					.route(web::get().to(payments_summary)),
			)
	})
	.keep_alive(Duration::from_secs(60))
	.bind(("0.0.0.0", 9999))?
	.run()
	.await
}
