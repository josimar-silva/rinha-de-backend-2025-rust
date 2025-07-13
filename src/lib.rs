use std::time::Duration;

use actix_web::{App, HttpResponse, HttpServer, web};
use anyhow::Result;
use chrono::{DateTime, Utc};
use log::{error, info, warn};
use redis::AsyncCommands;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio::time::sleep;
use uuid::Uuid;

// --- Retry Policy Constants ---
/// The maximum number of retry attempts for the default payment processor.
const MAX_DEFAULT_RETRY_ATTEMPTS: u32 = 3;
/// The initial delay for the exponential backoff strategy, in milliseconds.
const INITIAL_RETRY_DELAY_MS: u64 = 100;
/// The maximum possible delay between retries, in milliseconds.
const MAX_RETRY_DELAY_MS: u64 = 2000;

// --- Redis Keys ---
const PAYMENTS_QUEUE_KEY: &str = "payments_queue";
const PROCESSED_IDS_KEY: &str = "processed_correlation_ids";
const DEFAULT_HEALTH_KEY: &str = "payment_processor:default:healthy";
const SECONDARY_HEALTH_KEY: &str = "payment_processor:secondary:healthy";
const DEFAULT_SUMMARY_KEY: &str = "payments_summary_default";
const FALLBACK_SUMMARY_KEY: &str = "payments_summary_fallback";

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

/// Determines if a reqwest::Error is retryable.
///
/// Retryable errors are typically network-related issues like timeouts,
/// connection errors, or issues during the request phase that are not
/// related to the response body itself.
fn is_retryable_reqwest_error(e: &reqwest::Error) -> bool {
	e.is_connect() || e.is_timeout() || e.is_request()
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
		.lpush::<&str, String, ()>(PAYMENTS_QUEUE_KEY, payment_json)
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
		.hget(DEFAULT_SUMMARY_KEY, "totalRequests")
		.await
		.unwrap_or(0);
	let default_total_amount: f64 = con
		.hget(DEFAULT_SUMMARY_KEY, "totalAmount")
		.await
		.unwrap_or(0.0);

	let fallback_total_requests: i64 = con
		.hget(FALLBACK_SUMMARY_KEY, "totalRequests")
		.await
		.unwrap_or(0);
	let fallback_total_amount: f64 = con
		.hget(FALLBACK_SUMMARY_KEY, "totalAmount")
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
		let default_health = check_processor_health(&client, &default_url).await;
		let _: Result<(), _> = con.set(DEFAULT_HEALTH_KEY, default_health).await;
		info!(
			"Default processor health status updated: {}",
			if default_health {
				"Healthy"
			} else {
				"Unhealthy"
			}
		);

		// Check fallback payment processor
		let fallback_health = check_processor_health(&client, &fallback_url).await;
		let _: Result<(), _> = con.set(SECONDARY_HEALTH_KEY, fallback_health).await;
		info!(
			"Fallback processor health status updated: {}",
			if fallback_health {
				"Healthy"
			} else {
				"Unhealthy"
			}
		);

		sleep(Duration::from_secs(5)).await;
	}
}

async fn check_processor_health(client: &Client, base_url: &str) -> bool {
	let health_url =
		format!("{}/payments/service-health", base_url.trim_end_matches('/'));
	match client.get(&health_url).send().await {
		Ok(resp) => {
			if resp.status().is_success() {
				match resp.json::<HealthCheckResponse>().await {
					Ok(health) => !health.failing,
					Err(e) => {
						error!(
							"Failed to parse health check response from {}: {}",
							base_url, e
						);
						false
					}
				}
			} else {
				error!(
					"Health check for {} failed with status: {}",
					base_url,
					resp.status()
				);
				false
			}
		}
		Err(e) => {
			error!("Failed to reach health endpoint for {}: {}", base_url, e);
			false
		}
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
			match con.brpop(PAYMENTS_QUEUE_KEY, 0.0).await {
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
					"Failed to deserialize payment request from queue: {e}. 
					 Original string: {payment_str}"
				);
				continue; // Skip malformed messages
			}
		};

		// Check if correlation_id already processed
		let is_processed: bool = match con
			.sismember(PROCESSED_IDS_KEY, payment.correlation_id.to_string())
			.await
		{
			Ok(is_mem) => is_mem,
			Err(e) => {
				error!(
					"Failed to check processed_correlation_ids for {}: {e}",
					payment.correlation_id
				);
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

		let mut processed = false;

		// --- Stage 1: Attempt Default Processor with Retry Logic ---
		let is_default_healthy: bool =
			con.get(DEFAULT_HEALTH_KEY).await.unwrap_or(false);

		if is_default_healthy {
			let mut attempts = 0;
			let mut delay_ms = INITIAL_RETRY_DELAY_MS;

			while attempts < MAX_DEFAULT_RETRY_ATTEMPTS {
				attempts += 1;
				let req_body = PaymentProcessorRequest {
					correlation_id: payment.correlation_id,
					amount:         payment.amount,
					requested_at:   Utc::now(),
				};

				let res = client
					.post(format!("{}/payments", default_url.trim_end_matches('/')))
					.json(&req_body)
					.send()
					.await;

				match res {
					Ok(response) => {
						let status = response.status();
						if status.is_success() {
							info!(
								"Payment {} processed by default processor.",
								payment.correlation_id
							);
							let _ = update_redis_summary(
								&mut con,
								DEFAULT_SUMMARY_KEY,
								payment.amount,
								payment.correlation_id,
							)
							.await;
							processed = true;
							break; // Exit retry loop on success
						}

						if status.is_server_error() ||
							status == reqwest::StatusCode::TOO_MANY_REQUESTS
						{
							// FIX: Use reqwest::StatusCode
							warn!(
								"Default processor failed with retryable status \
								 {}. Attempt {}/{}",
								status, attempts, MAX_DEFAULT_RETRY_ATTEMPTS
							);
						} else {
							error!(
								"Default processor returned non-retryable status \
								 {}. Failing over.",
								status
							);
							break; // Exit loop for non-retryable HTTP errors
						}
					}
					Err(e) => {
						if is_retryable_reqwest_error(&e) {
							warn!(
								"Default processor network error: {}. Attempt {}/{}",
								e, attempts, MAX_DEFAULT_RETRY_ATTEMPTS
							);
						} else {
							error!(
								"Default processor non-retryable error: {}. \
								 Failing over.",
								e
							);
							break; // Exit loop for non-retryable reqwest errors
						}
					}
				}

				if attempts < MAX_DEFAULT_RETRY_ATTEMPTS {
					tokio::time::sleep(Duration::from_millis(delay_ms)).await;
					delay_ms = (delay_ms * 2).min(MAX_RETRY_DELAY_MS);
				}
			}
		} else {
			warn!("Default processor is marked as unhealthy. Skipping.");
		}

		// --- Stage 2: Fallback to Secondary Processor ---
		if !processed {
			let is_secondary_healthy: bool =
				con.get(SECONDARY_HEALTH_KEY).await.unwrap_or(false);
			if is_secondary_healthy {
				info!(
					"Attempting fallback to secondary processor for payment {}.",
					payment.correlation_id
				);
				let req_body = PaymentProcessorRequest {
					correlation_id: payment.correlation_id,
					amount:         payment.amount,
					requested_at:   Utc::now(),
				};
				match client
					.post(format!("{}/payments", fallback_url.trim_end_matches('/')))
					.json(&req_body)
					.send()
					.await
				{
					Ok(resp) if resp.status().is_success() => {
						info!(
							"Payment {} processed by fallback processor.",
							payment.correlation_id
						);
						let _ = update_redis_summary(
							&mut con,
							FALLBACK_SUMMARY_KEY,
							payment.amount,
							payment.correlation_id,
						)
						.await;
						processed = true;
					}
					Ok(resp) => {
						error!(
							"Fallback processor failed for {} with status: {}",
							payment.correlation_id,
							resp.status()
						);
					}
					Err(e) => {
						error!(
							"Failed to send payment {} to fallback processor: {}",
							payment.correlation_id, e
						);
					}
				}
			} else {
				warn!(
					"Fallback processor is also unhealthy. Cannot process payment \
					 {}.",
					payment.correlation_id
				);
			}
		}

		// --- Stage 3: Handle Unprocessed Payments ---
		if !processed {
			error!(
				"Payment {} could not be processed by any processor. Re-queueing.",
				payment.correlation_id
			);
			let _: Result<(), _> = con
				.lpush(PAYMENTS_QUEUE_KEY, serde_json::to_string(&payment).unwrap())
				.await;
		}
	}
}

async fn update_redis_summary(
	con: &mut redis::aio::MultiplexedConnection,
	summary_key: &str,
	amount: f64,
	correlation_id: Uuid,
) -> Result<(), redis::RedisError> {
	redis::pipe()
		.atomic()
		.hincr(summary_key, "totalRequests", 1)
		.hincr(summary_key, "totalAmount", amount)
		.sadd(PROCESSED_IDS_KEY, correlation_id.to_string())
		.query_async(con)
		.await
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
