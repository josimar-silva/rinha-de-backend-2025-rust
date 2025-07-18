use log::{error, info};
use reqwest::Client;
use tokio::time::{Duration, sleep};

use crate::domain::payment_processor::PaymentProcessor;
use crate::infrastructure::routing::in_memory_payment_router::InMemoryPaymentRouter;

pub async fn processor_health_monitor_worker(
	router: InMemoryPaymentRouter,
	http_client: Client,
	default_processor_url: String,
	fallback_processor_url: String,
) {
	let urls = [
		("default".to_string(), default_processor_url),
		("fallback".to_string(), fallback_processor_url),
	];

	loop {
		for (name, url) in &urls {
			let health_url = format!("{url}/payments/service-health");
			match http_client.get(&health_url).send().await {
				Ok(resp) => {
					if resp.status().is_success() {
						match resp.json::<serde_json::Value>().await {
							Ok(json) => {
								let failing =
									json["failing"].as_bool().unwrap_or(true);
								let min_response_time =
									json["minResponseTime"].as_i64().unwrap_or(0)
										as u64;

								let health_status = if failing {
									crate::domain::health_status::HealthStatus::Failing
								} else {
									crate::domain::health_status::HealthStatus::Healthy
								};

								let processor = PaymentProcessor {
									name: name.clone(),
									url: url.clone(),
									health: health_status.clone(),
									min_response_time,
								};
								router.update_processor_health(processor);
								info!(
									"Updated health for {name}: {:?}",
									health_status.clone()
								);
							}
							Err(e) => {
								error!(
									"Failed to parse health check response for \
									 {name}: {e}"
								);
							}
						}
					} else {
						error!(
							"Health check for {name}: {} returned non-success \
							 status",
							resp.status()
						);
					}
				}
				Err(e) => {
					error!("Failed to perform health check for {name}: {e}");
				}
			}
		}
		// Respect the 5-second rate limit for health checks
		sleep(Duration::from_secs(5)).await;
	}
}
