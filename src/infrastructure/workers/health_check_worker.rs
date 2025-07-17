use std::time::Duration;

use log::error;
use tokio::time::sleep;

use crate::use_cases::health_check::HealthCheckUseCase;

pub async fn health_check_worker<R>(
	health_check_use_case: HealthCheckUseCase<R>,
	default_url: String,
	fallback_url: String,
) where
	R: crate::domain::repository::PaymentProcessorRepository
		+ Clone
		+ Send
		+ Sync
		+ 'static,
{
	loop {
		if let Err(e) = health_check_use_case
			.execute("default".to_string(), default_url.clone())
			.await
		{
			error!("Error running health check for default processor: {e:?}");
		}

		if let Err(e) = health_check_use_case
			.execute("fallback".to_string(), fallback_url.clone())
			.await
		{
			error!("Error running health check for fallback processor: {e:?}");
		}

		sleep(Duration::from_secs(5)).await;
	}
}
