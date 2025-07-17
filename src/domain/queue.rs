use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Message<B> {
	pub id:   Uuid,
	pub body: B,
}

impl<B> Message<B> {
	pub fn with(id: Uuid, body: B) -> Message<B> {
		Message { id, body }
	}
}

#[async_trait]
pub trait Queue<B>: Send + Sync + 'static {
	async fn pop(
		&self,
	) -> Result<Option<Message<B>>, Box<dyn std::error::Error + Send>>;
	async fn push(
		&self,
		message: Message<B>,
	) -> Result<(), Box<dyn std::error::Error + Send>>;
}
