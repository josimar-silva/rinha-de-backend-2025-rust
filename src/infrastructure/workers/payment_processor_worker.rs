use std::time::Duration;

use log::{error, info, warn};
use tokio::time::sleep;

use crate::domain::payment::Payment;
use crate::domain::queue::Queue;
use crate::domain::repository::{PaymentProcessorRepository, PaymentRepository};
use crate::use_cases::process_payment::ProcessPaymentUseCase;

pub async fn payment_processing_worker<Q, PR, PPR>(
	queue: Q,
	payment_repo: PR,
	processor_repo: PPR,
	process_payment_use_case: ProcessPaymentUseCase<PR>,
	default_url: String,
	fallback_url: String,
) where
	Q: Queue<Payment> + Clone + Send + Sync + 'static,
	PR: PaymentRepository + Clone + Send + Sync + 'static,
	PPR: PaymentProcessorRepository + Clone + Send + Sync + 'static,
{
	loop {
		let message = match queue.pop().await {
			Ok(Some(val)) => val,
			Ok(None) => {
				info!("No payments in queue, waiting...");
				sleep(Duration::from_secs(1)).await;
				continue;
			}
			Err(e) => {
				error!("Failed to pop from payments queue: {e}");
				sleep(Duration::from_secs(1)).await;
				continue;
			}
		};

		let message_id = message.id;

		info!("Started processing message with id '{}'", &message_id);

		let payment: Payment = message.body.clone();

		if let Ok(true) = payment_repo
			.is_already_processed(&payment.correlation_id.to_string())
			.await
		{
			info!("Payment already processed. Skipping it.");
			continue;
		}

		let default_failing =
			is_backend_failing_or_slow("default", &processor_repo).await;

		let mut processed = false;

		if !default_failing {
			processed = process_payment_use_case
				.execute(payment.clone(), default_url.clone(), "default".to_string())
				.await
				.unwrap_or(false);
		}

		let fallback_failing =
			is_backend_failing_or_slow("fallback", &processor_repo).await;

		if !processed && !fallback_failing {
			processed = process_payment_use_case
				.execute(
					payment.clone(),
					fallback_url.clone(),
					"fallback".to_string(),
				)
				.await
				.unwrap_or(false);
		}

		if !processed {
			warn!(
				"Payment {} could not be processed by any processor. Re-queueing.",
				payment.correlation_id
			);
			if let Err(e) = queue.push(message).await {
				error!("Failed to re-queue payment: {e}");
			}
		}

		info!("Message with id '{}' processed.", &message_id);
	}
}

async fn is_backend_failing_or_slow<PPR>(
	processor_name: &str,
	processor_repo: &PPR,
) -> bool
where
	PPR: PaymentProcessorRepository + Clone + Send + Sync + 'static,
{
	processor_repo
		.get_health_of(processor_name)
		.await
		.unwrap_or(1i32) !=
		0
}
