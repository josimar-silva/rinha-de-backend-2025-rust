use crate::domain::payment::Payment;
use crate::domain::queue::{Message, Queue};
use crate::use_cases::dto::CreatePaymentCommand;

#[derive(Clone)]
pub struct CreatePaymentUseCase<Q: Queue<Payment>> {
	payment_queue: Q,
}

impl<Q: Queue<Payment>> CreatePaymentUseCase<Q> {
	pub fn new(payment_queue: Q) -> Self {
		Self { payment_queue }
	}

	pub async fn execute(
		&self,
		command: CreatePaymentCommand,
	) -> Result<(), Box<dyn std::error::Error + Send>> {
		let payment = Payment {
			correlation_id: command.correlation_id,
			amount:         command.amount,
			requested_at:   None,
			processed_at:   None,
			processed_by:   None,
		};

		self.payment_queue
			.push(Message::with(command.correlation_id, payment))
			.await
	}
}
