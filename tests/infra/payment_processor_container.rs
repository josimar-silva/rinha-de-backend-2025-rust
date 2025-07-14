use testcontainers::GenericImage;
use testcontainers::core::wait::HttpWaitStrategy;
use testcontainers::core::{ContainerPort, WaitFor};
use testcontainers::runners::AsyncRunner;

pub async fn setup_payment_processors() -> (
	String,
	String,
	testcontainers::ContainerAsync<GenericImage>,
	testcontainers::ContainerAsync<GenericImage>,
) {
	let default_processor_container =
		GenericImage::new("zanfranceschi/payment-processor", "latest")
			.with_exposed_port(ContainerPort::Tcp(8080))
			.with_wait_for(WaitFor::http(
				HttpWaitStrategy::new("/").with_expected_status_code(200_u16),
			))
			.start()
			.await
			.unwrap();

	let fallback_processor_container =
		GenericImage::new("zanfranceschi/payment-processor", "latest")
			.with_exposed_port(testcontainers::core::ContainerPort::Tcp(8080))
			.with_wait_for(WaitFor::http(
				HttpWaitStrategy::new("/").with_expected_status_code(200_u16),
			))
			.start()
			.await
			.unwrap();

	let default_port = default_processor_container.get_host_port_ipv4(8080).await;
	let fallback_port = fallback_processor_container.get_host_port_ipv4(8080).await;

	let default_url = format!("http://127.0.0.1:{}", default_port.unwrap());
	let fallback_url = format!("http://127.0.0.1:{}", fallback_port.unwrap());

	(
		default_url,
		fallback_url,
		default_processor_container,
		fallback_processor_container,
	)
}
