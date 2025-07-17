use async_trait::async_trait;
use redis::{AsyncCommands, Client};

use crate::domain::payment::Payment;
use crate::domain::queue::{Message, Queue};
use crate::infrastructure::config::redis::PAYMENTS_QUEUE_KEY;

#[derive(Clone)]
pub struct PaymentQueue {
	client: Client,
}

impl PaymentQueue {
	pub fn new(client: Client) -> Self {
		Self { client }
	}
}

#[async_trait]
impl Queue<Payment> for PaymentQueue {
	async fn pop(
		&self,
	) -> Result<Option<Message<Payment>>, Box<dyn std::error::Error + Send>> {
		let mut con = self
			.client
			.get_multiplexed_async_connection()
			.await
			.map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send>)?;

		let popped_value: Option<(String, String)> = con
			.brpop(PAYMENTS_QUEUE_KEY, 1.0)
			.await
			.map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send>)?;

		let message_str =
			if let Some((_queue_name, serialized_message)) = popped_value {
				serialized_message
			} else {
				return Ok(None);
			};

		let message: Message<Payment> = serde_json::from_str(&message_str)
			.map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send>)?;

		Ok(Some(message))
	}

	async fn push(
		&self,
		message: Message<Payment>,
	) -> Result<(), Box<dyn std::error::Error + Send>> {
		let mut con = self
			.client
			.get_multiplexed_async_connection()
			.await
			.map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send>)?;

		let serialized_message = serde_json::to_string(&message)
			.map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send>)?;

		let _: () = con
			.lpush(PAYMENTS_QUEUE_KEY, serialized_message)
			.await
			.map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send>)?;
		Ok(())
	}
}
