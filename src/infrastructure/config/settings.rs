use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
	pub redis_url: String,
	pub default_payment_processor_url: String,
	pub fallback_payment_processor_url: String,
	pub server_keepalive: u64,
	pub report_url: Option<String>,
}

impl Config {
	pub fn load() -> Result<Self, config::ConfigError> {
		let config_builder = config::Config::builder()
			.add_source(config::Environment::with_prefix("APP"))
			.build()?;

		config_builder.try_deserialize()
	}
}

#[cfg(test)]
mod tests {
	use std::env;

	use super::*;

	#[test]
	fn test_config_load() {
		unsafe {
			env::set_var("APP_REDIS_URL", "redis://test_redis/");
			env::set_var(
				"APP_DEFAULT_PAYMENT_PROCESSOR_URL",
				"http://test_default/",
			);
			env::set_var(
				"APP_FALLBACK_PAYMENT_PROCESSOR_URL",
				"http://test_fallback/",
			);
			env::set_var("APP_SERVER_KEEPALIVE", "120");
			env::set_var("APP_REPORT_URL", "/tmp/reports");
		};

		let config = Config::load().expect("Failed to load config in test");

		assert_eq!(config.redis_url, "redis://test_redis/");
		assert_eq!(config.default_payment_processor_url, "http://test_default/");
		assert_eq!(
			config.fallback_payment_processor_url,
			"http://test_fallback/"
		);
		assert_eq!(config.server_keepalive, 120);
		assert_eq!(config.report_url, Some("/tmp/reports".to_string()));

		unsafe {
			env::remove_var("APP_REDIS_URL");
			env::remove_var("APP_DEFAULT_PAYMENT_PROCESSOR_URL");
			env::remove_var("APP_FALLBACK_PAYMENT_PROCESSOR_URL");
			env::remove_var("APP_SERVER_KEEPALIVE");
			env::remove_var("APP_REPORT_URL");
		}
	}

	#[test]
	fn test_config_load_without_report_url() {
		unsafe {
			env::set_var("APP_REDIS_URL", "redis://test_redis_no_report/");
			env::set_var(
				"APP_DEFAULT_PAYMENT_PROCESSOR_URL",
				"http://test_default_no_report/",
			);
			env::set_var(
				"APP_FALLBACK_PAYMENT_PROCESSOR_URL",
				"http://test_fallback_no_report/",
			);
			env::set_var("APP_SERVER_KEEPALIVE", "120");
			env::remove_var("APP_REPORT_URL");
		};

		let config = Config::load().expect("Failed to load config in test");

		assert_eq!(config.redis_url, "redis://test_redis_no_report/");
		assert_eq!(
			config.default_payment_processor_url,
			"http://test_default_no_report/"
		);
		assert_eq!(
			config.fallback_payment_processor_url,
			"http://test_fallback_no_report/"
		);
		assert_eq!(config.server_keepalive, 120);
		assert_eq!(config.report_url, None);

		unsafe {
			env::remove_var("APP_REDIS_URL");
			env::remove_var("APP_DEFAULT_PAYMENT_PROCESSOR_URL");
			env::remove_var("APP_FALLBACK_PAYMENT_PROCESSOR_URL");
			env::remove_var("APP_SERVER_KEEPALIVE");
		}
	}
}
