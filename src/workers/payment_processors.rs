use std::time::Duration;

use anyhow::Result;
use chrono::{DateTime, Utc};
use log::{error, info};
use redis::AsyncCommands;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio::time::sleep;
use uuid::Uuid;

#[derive(Debug, Deserialize, Serialize)]
pub struct PaymentProcessorRequest {
	pub correlation_id: Uuid,
	pub amount:         f64,
	#[serde(with = "chrono::serde::ts_seconds")]
	pub requested_at:   DateTime<Utc>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct PaymentRequest {
	pub correlation_id: Uuid,
	pub amount:         f64,
}

#[derive(Debug, Deserialize)]
pub struct HealthCheckResponse {
	pub failing:           bool,
	#[serde(rename = "minResponseTime")]
	pub min_response_time: u64,
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
				sleep(Duration::from_secs(3)).await;
				continue;
			}
		};

		update_processor_health("default", &client, &default_url, &mut con).await;

		update_processor_health("fallback", &client, &fallback_url, &mut con).await;

		sleep(Duration::from_secs(5)).await;
	}
}

async fn update_processor_health(
	processor: &str,
	client: &Client,
	processor_url: &str,
	con: &mut redis::aio::MultiplexedConnection,
) {
	let processor_health_key = format!("health:{processor}");

	match client
		.get(format!("{processor_url}/payments/service-health"))
		.send()
		.await
	{
		Ok(resp) => {
			if resp.status().is_success() {
				match resp.json::<HealthCheckResponse>().await {
					Ok(health) => {
						let _: Result<(), _> = con
							.hset_multiple(processor_health_key, &[
								("failing", (health.failing as i32).to_string()),
								(
									"min_response_time",
									health.min_response_time.to_string(),
								),
							])
							.await;
						info!(
							"{processor} processor health: failing={}, \
							 min_response_time={}",
							health.failing, health.min_response_time
						);
					}
					Err(e) => {
						error!(
							"Failed to parse {processor} health check response: {e}"
						);
						let _: Result<(), _> =
							con.hset(processor_health_key, "failing", "1").await;
					}
				}
			} else {
				error!(
					"{processor} processor health check failed with status: {}",
					resp.status()
				);
				let _: Result<(), _> =
					con.hset(processor_health_key, "failing", "1").await;
			}
		}
		Err(e) => {
			error!("Failed to reach {processor} payment processor: {e}");
			let _: Result<(), _> =
				con.hset(processor_health_key, "failing", "1").await;
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
			con.hget("health:default", "failing").await.unwrap_or(1i32) != 0;

		let fallback_failing: bool =
			con.hget("health:fallback", "failing").await.unwrap_or(1i32) != 0;

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
							.query_async::<i64>(&mut con)
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
							.query_async::<f64>(&mut con)
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
							.query_async::<i64>(&mut con)
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
							.query_async::<f64>(&mut con)
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
