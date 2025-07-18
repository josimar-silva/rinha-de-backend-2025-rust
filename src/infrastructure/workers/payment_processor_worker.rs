use std::time::Duration;

use circuitbreaker_rs::State;
use log::{error, info, warn};
use tokio::time::sleep;

use crate::domain::payment::Payment;
use crate::domain::payment_router::PaymentRouter;
use crate::domain::queue::Queue;
use crate::domain::repository::PaymentRepository;
use crate::use_cases::process_payment::ProcessPaymentUseCase;

pub async fn payment_processing_worker<Q, PR, R>(
	queue: Q,
	payment_repo: PR,
	process_payment_use_case: ProcessPaymentUseCase<PR>,
	router: R,
) where
	Q: Queue<Payment> + Clone + Send + Sync + 'static,
	PR: PaymentRepository + Clone + Send + Sync + 'static,
	R: PaymentRouter + Clone + Send + Sync + 'static,
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

		let mut processed = false;

		if let Some((processor_url, processor_name, circuit_breaker)) =
			router.get_processor_for_payment().await
		{
			if circuit_breaker.current_state() == State::Open {
				warn!(
					"Circuit breaker for {processor_name} is open. Skipping \
					 payment processing and re-queueing."
				);
				if let Err(e) = queue.push(message).await {
					error!("Failed to re-queue payment: {e}");
				}
				continue;
			}

			processed = process_payment_use_case
				.execute(
					payment.clone(),
					processor_url,
					processor_name,
					circuit_breaker,
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
