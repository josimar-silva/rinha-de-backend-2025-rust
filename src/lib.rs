use std::sync::Arc;
use std::time::Duration;

use actix_web::{App, HttpServer, web};
use log::info;
use reqwest::Client;

pub mod api;
pub mod config;
pub mod model;
pub mod workers;

use crate::api::handlers::{payments, payments_summary};
use crate::workers::health_check_worker::*;
use crate::workers::payment_processor_worker::*;

pub async fn run(config: Arc<config::Config>) -> std::io::Result<()> {
	env_logger::init();

	let redis_client =
		redis::Client::open(config.redis_url.clone()).expect("Invalid Redis URL");

	let http_client = Client::new();

	info!("Starting health check worker...");
	tokio::spawn(health_check_worker(
		redis_client.clone(),
		http_client.clone(),
		config.default_payment_processor_url.clone(),
		config.fallback_payment_processor_url.clone(),
	));

	info!("Starting payment processing worker...");
	tokio::spawn(payment_processing_worker(
		redis_client.clone(),
		http_client.clone(),
		config.default_payment_processor_url.clone(),
		config.fallback_payment_processor_url.clone(),
	));

	info!("Starting Actix-Web server on 0.0.0.0:9999...");
	HttpServer::new(move || {
		App::new()
			.app_data(web::Data::new(redis_client.clone()))
			.service(payments)
			.service(payments_summary)
	})
	.keep_alive(Duration::from_secs(config.server_keepalive))
	.bind(("0.0.0.0", 9999))?
	.run()
	.await
}
