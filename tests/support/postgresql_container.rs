use log::info;
use testcontainers::core::{ContainerPort, Mount, WaitFor};
use testcontainers::runners::AsyncRunner;
use testcontainers::{GenericImage, ImageExt};
use uuid::Uuid;

pub struct PostgresTestContainer {
	pub database_url: String,
	pub container:    testcontainers::ContainerAsync<GenericImage>,
}

pub async fn setup_postgresql_container() -> PostgresTestContainer {
	let database_name = "payment_processor";
	let database_user = "payment-processor-user";
	let database_password = "payment-processor-user";

	let container_name = format!("payment-processor-db-{}", Uuid::new_v4());

	let container = GenericImage::new("postgres", "17-alpine")
		.with_wait_for(WaitFor::message_on_stdout(
			"database system is ready to accept connections",
		))
		.with_exposed_port(ContainerPort::Tcp(5432))
		.with_container_name(container_name.clone())
		.with_network("test-network") // Use a named network
		.with_env_var("POSTGRES_DB", database_name)
		.with_env_var("POSTGRES_USER", database_user)
		.with_env_var("POSTGRES_PASSWORD", database_password)
		.with_mount(Mount::bind_mount(
			format!("{}/payment-processor/init.sql", env!("CARGO_MANIFEST_DIR")),
			"/docker-entrypoint-initdb.d/init.sql".to_string(),
		))
		.start()
		.await
		.unwrap();

	let database_url = format!(
		"Host={};Port={};Database={database_name};Username={database_user};\
		 Password={database_user};Minimum Pool Size=15; Maximum Pool \
		 Size=20;Connection Pruning Interval=3",
		container_name, 5432
	);

	info!("Postgres Container running at {database_url}");

	PostgresTestContainer {
		database_url,
		container,
	}
}
