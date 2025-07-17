use redis::AsyncCommands;

mod support;

use crate::support::payment_processor_container::setup_payment_processors;
use crate::support::postgresql_container::setup_postgresql_container;
use crate::support::redis_container::get_test_redis_client;

#[tokio::test]
async fn test_postgresql_container() {
	let postgresql_container = setup_postgresql_container().await;

	assert!(!postgresql_container.database_url.is_empty());
	assert!(!postgresql_container.container.id().is_empty());
}

#[tokio::test]
async fn test_payment_processor_container() {
	let (default_processor_container, fallback_processor_container) =
		setup_payment_processors().await;

	assert!(!default_processor_container.url.is_empty());
	assert!(!default_processor_container.container.id().is_empty());
	assert!(!fallback_processor_container.url.is_empty());
	assert!(!fallback_processor_container.container.id().is_empty());
}

#[tokio::test]
async fn test_redis_container() {
	let redis_container = get_test_redis_client().await;

	assert!(!redis_container.container.id().is_empty());

	let mut con = redis_container
		.client
		.get_multiplexed_async_connection()
		.await
		.unwrap();
	let _: () = con.set("test_key", "test_value").await.unwrap();
	let value: String = con.get("test_key").await.unwrap();

	assert_eq!(value, "test_value");
}
