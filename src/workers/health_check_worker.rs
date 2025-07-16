use std::time::Duration;

use anyhow::Result;
use log::{error, info};
use redis::AsyncCommands;
use reqwest::Client;
use tokio::time::sleep;

use crate::model::payment_processor::HealthCheckResponse;

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
