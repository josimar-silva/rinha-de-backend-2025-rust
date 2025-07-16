#[cfg(test)]
#[actix_web::test]
async fn test_run_bind_error() {
	let listener = std::net::TcpListener::bind("0.0.0.0:9999").unwrap();
	assert!(rinha_de_backend::run().await.is_err());
	drop(listener);
}
