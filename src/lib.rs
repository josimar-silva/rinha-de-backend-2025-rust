use std::time::Duration;

use actix_web::{App, HttpResponse, HttpServer, web};
use anyhow::Result;
use chrono::{DateTime, Utc};
use log::{error, info};
use redis::AsyncCommands;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio::time::sleep;
use uuid::Uuid;

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct PaymentRequest {
	pub correlation_id: Uuid,
	pub amount:         f64,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct PaymentProcessorRequest {
	pub correlation_id: Uuid,
	pub amount:         f64,
	#[serde(with = "chrono::serde::ts_seconds")]
	pub requested_at:   DateTime<Utc>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct PaymentsSummaryResponse {
	pub default:  SummaryData,
	pub fallback: SummaryData,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct SummaryData {
	#[serde(rename = "totalRequests")]
	pub total_requests: i64,
	#[serde(rename = "totalAmount")]
	pub total_amount:   f64,
}

#[derive(Debug, Deserialize)]
pub struct HealthCheckResponse {
	pub failing:           bool,
	#[serde(rename = "minResponseTime")]
	pub min_response_time: u64,
}

pub async fn payments(
	req: web::Json<PaymentRequest>,
	redis_client: web::Data<redis::Client>,
) -> HttpResponse {
	let mut con = match redis_client.get_multiplexed_async_connection().await {
		Ok(con) => con,
		Err(e) => {
			error!("Failed to get Redis connection: {e}");
			return HttpResponse::InternalServerError()
				.body("Internal Server Error");
		}
	};

	let payment_json = match serde_json::to_string(&req.0) {
		Ok(json) => json,
		Err(e) => {
			error!("Failed to serialize payment request: {e}");
			return HttpResponse::InternalServerError()
				.body("Internal Server Error");
		}
	};

	match con
		.lpush::<&str, String, ()>("payments_queue", payment_json)
		.await
	{
		Ok(_) => {
			info!("Payment received and queued: {}", req.correlation_id);
			HttpResponse::Ok().body("Payment received")
		}
		Err(e) => {
			error!("Failed to push payment to Redis queue: {e}");
			HttpResponse::InternalServerError().body("Internal Server Error")
		}
	}
}

pub async fn payments_summary(
	redis_client: web::Data<redis::Client>,
) -> HttpResponse {
	let mut con = match redis_client.get_multiplexed_async_connection().await {
		Ok(con) => con,
		Err(e) => {
			error!("Failed to get Redis connection for summary: {e}");
			return HttpResponse::InternalServerError()
				.body("Internal Server Error");
		}
	};

	let default_total_requests: i64 = con
		.hget("payments_summary_default", "totalRequests")
		.await
		.unwrap_or(0);
	let default_total_amount: f64 = con
		.hget("payments_summary_default", "totalAmount")
		.await
		.unwrap_or(0.0);

	let fallback_total_requests: i64 = con
		.hget("payments_summary_fallback", "totalRequests")
		.await
		.unwrap_or(0);
	let fallback_total_amount: f64 = con
		.hget("payments_summary_fallback", "totalAmount")
		.await
		.unwrap_or(0.0);

	let response = PaymentsSummaryResponse {
		default:  SummaryData {
			total_requests: default_total_requests,
			total_amount:   default_total_amount,
		},
		fallback: SummaryData {
			total_requests: fallback_total_requests,
			total_amount:   fallback_total_amount,
		},
	};

	HttpResponse::Ok().json(response)
}

pub async fn health_check_worker(
	redis_client: redis::Client,
	client: Client,
	default_url: String,
	fallback_url: String,
) {
	loop {
		let mut con = match redis_client.get_multiplexed_async_connection().await {
			Ok(con) => con,
			Err(e) => {
				error!("Health check worker failed to get Redis connection: {e}");
				sleep(Duration::from_secs(5)).await;
				continue;
			}
		};

		// Check default payment processor
		match client
			.get(format!("{default_url}/payments/service-health"))
			.send()
			.await
		{
			Ok(resp) => {
				if resp.status().is_success() {
					match resp.json::<HealthCheckResponse>().await {
						Ok(health) => {
							let _: Result<(), _> = con
								.set("health_default_failing", health.failing)
								.await;
							let _: Result<(), _> = con
								.set(
									"health_default_min_response_time",
									health.min_response_time,
								)
								.await;
							info!(
								"Default processor health: failing={}, \
								 min_response_time={}",
								health.failing, health.min_response_time
							);
						}
						Err(e) => {
							error!(
								"Failed to parse default health check response: {e}"
							);
							let _: Result<(), _> =
								con.set("health_default_failing", true).await;
						}
					}
				} else {
					error!(
						"Default processor health check failed with status: {}",
						resp.status()
					);
					let _: Result<(), _> =
						con.set("health_default_failing", true).await;
				}
			}
			Err(e) => {
				error!("Failed to reach default payment processor: {e}");
				let _: Result<(), _> = con.set("health_default_failing", true).await;
			}
		}

		// Check fallback payment processor
		match client
			.get(format!("{fallback_url}/payments/service-health"))
			.send()
			.await
		{
			Ok(resp) => {
				if resp.status().is_success() {
					match resp.json::<HealthCheckResponse>().await {
						Ok(health) => {
							let _: Result<(), _> = con
								.set("health_fallback_failing", health.failing)
								.await;
							let _: Result<(), _> = con
								.set(
									"health_fallback_min_response_time",
									health.min_response_time,
								)
								.await;
							info!(
								"Fallback processor health: failing={}, \
								 min_response_time={}",
								health.failing, health.min_response_time
							);
						}
						Err(e) => {
							error!(
								"Failed to parse fallback health check response: \
								 {e}"
							);
							let _: Result<(), _> =
								con.set("health_fallback_failing", true).await;
						}
					}
				} else {
					error!(
						"Fallback processor health check failed with status: {}",
						resp.status()
					);
					let _: Result<(), _> =
						con.set("health_fallback_failing", true).await;
				}
			}
			Err(e) => {
				error!("Failed to reach fallback payment processor: {e}");
				let _: Result<(), _> =
					con.set("health_fallback_failing", true).await;
			}
		}

		sleep(Duration::from_secs(5)).await;
	}
}

pub async fn payment_processing_worker(
	redis_client: redis::Client,
	client: Client,
	default_url: String,
	fallback_url: String,
) {
	loop {
		let mut con = match redis_client.get_multiplexed_async_connection().await {
			Ok(con) => con,
			Err(e) => {
				error!(
					"Payment processing worker failed to get Redis connection: {e}"
				);
				sleep(Duration::from_secs(1)).await;
				continue;
			}
		};

		let popped_value: Option<(String, String)> =
			match con.brpop("payments_queue", 0.0).await {
				Ok(val) => val,
				Err(e) => {
					error!("Failed to pop from payments queue: {e}");
					sleep(Duration::from_secs(1)).await;
					continue;
				}
			};

		let payment_str = if let Some((_key, val)) = popped_value {
			info!("Payment dequeued: {val:?}");
			val
		} else {
			info!("No payments in queue, waiting...");
			sleep(Duration::from_secs(1)).await;
			continue;
		};

		let payment: PaymentRequest = match serde_json::from_str(&payment_str) {
			Ok(p) => p,
			Err(e) => {
				error!(
					"Failed to deserialize payment request from queue: {e}. \
					 Original string: {payment_str}"
				);
				continue; // Skip malformed messages
			}
		};

		// Check if correlation_id already processed
		let is_processed: bool = match con
			.sismember(
				"processed_correlation_ids",
				payment.correlation_id.to_string(),
			)
			.await
		{
			Ok(is_mem) => is_mem,
			Err(e) => {
				error!(
					"Failed to check processed_correlation_ids for {}: {e}",
					payment.correlation_id
				);
				// TODO: Decide how to handle: retry, or process anyway (risk of
				// duplicate) For now, we'll assume it's not processed to avoid
				// blocking.
				false
			}
		};

		if is_processed {
			info!(
				"Skipping already processed payment: {}",
				payment.correlation_id
			);
			continue;
		}

		let default_failing: bool =
			con.get("health_default_failing").await.unwrap_or(true);
		let fallback_failing: bool =
			con.get("health_fallback_failing").await.unwrap_or(true);

		let mut processed = false;

		// Try default first
		if !default_failing {
			let req_body = PaymentProcessorRequest {
				correlation_id: payment.correlation_id,
				amount:         payment.amount,
				requested_at:   Utc::now(),
			};
			match client
				.post(format!("{default_url}/payments"))
				.json(&req_body)
				.send()
				.await
			{
				Ok(resp) => {
					if resp.status().is_success() {
						info!(
							"Payment {} processed by default processor. Updating \
							 Redis summary.",
							payment.correlation_id
						);
						match redis::cmd("HINCRBY")
							.arg("payments_summary_default")
							.arg("totalRequests")
							.arg(1)
							.query_async::<redis::aio::MultiplexedConnection, ()>(
								&mut con,
							)
							.await
						{
							Ok(_) => {
								info!(
									"Successfully HINCRBY totalRequests for \
									 default processor."
								)
							}
							Err(e) => error!(
								"Failed to HINCRBY totalRequests for default \
								 processor: {e}"
							),
						}
						match redis::cmd("HINCRBYFLOAT")
							.arg("payments_summary_default")
							.arg("totalAmount")
							.arg(payment.amount)
							.query_async::<redis::aio::MultiplexedConnection, ()>(
								&mut con,
							)
							.await
						{
							Ok(_) => info!(
								"Successfully HINCRBYFLOAT totalAmount for default \
								 processor."
							),
							Err(e) => error!(
								"Failed to HINCRBYFLOAT totalAmount for default \
								 processor: {e}"
							),
						}
						match con
							.sadd::<&str, String, ()>(
								"processed_correlation_ids",
								payment.correlation_id.to_string(),
							)
							.await
						{
							Ok(_) => info!(
								"Successfully added {} to \
								 processed_correlation_ids.",
								payment.correlation_id
							),
							Err(e) => error!(
								"Failed to add {} to processed_correlation_ids: {e}",
								payment.correlation_id
							),
						}
						processed = true;
					} else {
						error!(
							"Default processor returned non-success status for {}: \
							 {}",
							payment.correlation_id,
							resp.status()
						);
					}
				}
				Err(e) => {
					error!(
						"Failed to send payment {} to default processor: {e}",
						payment.correlation_id
					);
				}
			}
		}

		// If default failed or was failing, try fallback
		if !processed && !fallback_failing {
			let req_body = PaymentProcessorRequest {
				correlation_id: payment.correlation_id,
				amount:         payment.amount,
				requested_at:   Utc::now(),
			};
			match client
				.post(format!("{fallback_url}/payments"))
				.json(&req_body)
				.send()
				.await
			{
				Ok(resp) => {
					if resp.status().is_success() {
						info!(
							"Payment {} processed by fallback processor. Updating \
							 Redis summary.",
							payment.correlation_id
						);
						match redis::cmd("HINCRBY")
							.arg("payments_summary_fallback")
							.arg("totalRequests")
							.arg(1)
							.query_async::<redis::aio::MultiplexedConnection, ()>(
								&mut con,
							)
							.await
						{
							Ok(_) => {
								info!(
									"Successfully HINCRBY totalRequests for \
									 fallback processor."
								)
							}
							Err(e) => error!(
								"Failed to HINCRBY totalRequests for fallback \
								 processor: {e}"
							),
						}
						match redis::cmd("HINCRBYFLOAT")
							.arg("payments_summary_fallback")
							.arg("totalAmount")
							.arg(payment.amount)
							.query_async::<redis::aio::MultiplexedConnection, ()>(
								&mut con,
							)
							.await
						{
							Ok(_) => info!(
								"Successfully HINCRBYFLOAT totalAmount for \
								 fallback processor."
							),
							Err(e) => error!(
								"Failed to HINCRBYFLOAT totalAmount for fallback \
								 processor: {e}"
							),
						}
						match con
							.sadd::<&str, String, ()>(
								"processed_correlation_ids",
								payment.correlation_id.to_string(),
							)
							.await
						{
							Ok(_) => info!(
								"Successfully added {} to \
								 processed_correlation_ids.",
								payment.correlation_id
							),
							Err(e) => error!(
								"Failed to add {} to processed_correlation_ids: {e}",
								payment.correlation_id
							),
						}
						processed = true;
					} else {
						error!(
							"Fallback processor returned non-success status for \
							 {}: {}",
							payment.correlation_id,
							resp.status()
						);
					}
				}
				Err(e) => {
					error!(
						"Failed to send payment {} to fallback processor: {}",
						payment.correlation_id, e
					);
				}
			}
		}

		// If still not processed, push back to queue or handle as failed
		if !processed {
			error!(
				"Payment {} could not be processed by any processor. Re-queueing.",
				payment.correlation_id
			);
			let _: Result<(), _> = con
				.lpush("payments_queue", serde_json::to_string(&payment).unwrap())
				.await;
		}
	}
}

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
	.bind(("0.0.0.0", 9999))?
	.run()
	.await
}
