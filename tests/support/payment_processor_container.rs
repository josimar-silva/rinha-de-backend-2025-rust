use testcontainers::core::wait::HttpWaitStrategy;
use testcontainers::core::{ContainerPort, WaitFor};
use testcontainers::runners::AsyncRunner;
use testcontainers::{GenericImage, ImageExt};

use crate::support::postgresql_container::{
	PostgresTestContainer, setup_postgresql_container,
};

pub async fn setup_payment_processors()
-> (PaymentProcessorTestContainer, PaymentProcessorTestContainer) {
	let default_processor_container = setup_payment_processor(0.05, 5).await;

	let fallback_processor_container = setup_payment_processor(0.15, 5).await;

	(default_processor_container, fallback_processor_container)
}

pub struct PaymentProcessorTestContainer {
	pub url:       String,
	pub container: testcontainers::ContainerAsync<GenericImage>,
	pub database:  PostgresTestContainer,
}

async fn setup_payment_processor(
	transaction_fee: f64,
	rate_limit: i8,
) -> PaymentProcessorTestContainer {
	let database_container = setup_postgresql_container().await;
	let database_url = database_container.database_url.clone();

	let payment_processor_container =
		GenericImage::new("zanfranceschi/payment-processor", "amd64-20250707101540")
			.with_wait_for(WaitFor::http(
				HttpWaitStrategy::new("/").with_expected_status_code(200_u16),
			))
			.with_exposed_port(ContainerPort::Tcp(8080))
			.with_network("test-network")
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

	PaymentProcessorTestContainer {
		url:       container_url,
		container: payment_processor_container,
		database:  database_container,
	}
}
