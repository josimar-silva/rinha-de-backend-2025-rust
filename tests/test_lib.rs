use std::sync::Arc;

use rinha_de_backend::config::Config;

#[cfg(test)]
#[actix_web::test]
async fn test_run_bind_error() {
	let listener = std::net::TcpListener::bind("0.0.0.0:9999").unwrap();

	let dummy_config = Arc::new(Config {
		redis_url: "redis://127.0.0.1/".to_string(),
		default_payment_processor_url: "http://localhost:8080".to_string(),
		fallback_payment_processor_url: "http://localhost:8081".to_string(),
		server_keepalive: 60,
	});

	assert!(rinha_de_backend::run(dummy_config).await.is_err());
	drop(listener);
}
