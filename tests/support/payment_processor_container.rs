use testcontainers::core::wait::HttpWaitStrategy;
use testcontainers::core::{ContainerPort, WaitFor};
use testcontainers::runners::AsyncRunner;
use testcontainers::{GenericImage, ImageExt};

use crate::support::postgresql_container::setup_postgresql_container;

#[allow(dead_code)]
pub async fn setup_payment_processors() -> (
	String,
	String,
	testcontainers::ContainerAsync<GenericImage>,
	testcontainers::ContainerAsync<GenericImage>,
) {
	let (default_database_url, _database_container) =
		setup_postgresql_container().await;

	let (default_url, default_processor_container) =
		setup_payment_processor(0.05, 5, default_database_url.clone()).await;

	let (fallback_database_url, _database_container) =
		setup_postgresql_container().await;

	let (fallback_url, fallback_processor_container) =
		setup_payment_processor(0.15, 5, fallback_database_url).await;

	(
		default_url,
		fallback_url,
		default_processor_container,
		fallback_processor_container,
	)
}

async fn setup_payment_processor(
	transaction_fee: f64,
	rate_limit: i8,
	database_url: String,
) -> (String, testcontainers::ContainerAsync<GenericImage>) {
	let payment_processor_container =
		GenericImage::new("zanfranceschi/payment-processor", "amd64-20250707101540")
			.with_wait_for(WaitFor::http(
				HttpWaitStrategy::new("/").with_expected_status_code(200_u16),
			))
			.with_exposed_port(ContainerPort::Tcp(8080))
			.with_network("test-network") // Use a named network
			.with_env_var("DB_CONNECTION_STRING", database_url)
			.with_env_var("TRANSACTION_FEE", transaction_fee.to_string())
			.with_env_var("RATE_LIMIT_SECONDS", rate_limit.to_string())
			.with_env_var("INITIAL_TOKEN", "123")
			.start()
			.await
			.unwrap();

	let container_host = payment_processor_container.get_host().await;
	let container_port = payment_processor_container.get_host_port_ipv4(8080).await;
	let container_url = format!(
		"http://{}:{}",
		container_host.unwrap(),
		container_port.unwrap()
	);

	(container_url, payment_processor_container)
}
