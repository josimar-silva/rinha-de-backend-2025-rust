use std::sync::Arc;

use rinha_de_backend::infrastructure::config::settings::Config;
use rinha_de_backend::run;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
	let config = Arc::new(Config::load().expect("Failed to load configuration"));
	run(config).await
}
