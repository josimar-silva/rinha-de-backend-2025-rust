use std::sync::Arc;

#[cfg(feature = "perf")]
use pprof::flamegraph::Options;
use rinha_de_backend::infrastructure::config::settings::Config;
use rinha_de_backend::run;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
	#[cfg(feature = "perf")]
	let guard = pprof::ProfilerGuardBuilder::default()
		.frequency(1000)
		.blocklist(&["libc", "libgcc", "pthread", "vdso"])
		.build()
		.unwrap();

	let config = Arc::new(Config::load().expect("Failed to load configuration"));
	let result = run(config.clone()).await;

	#[cfg(feature = "perf")]
	if let Ok(report) = guard.report().build() {
		if let Some(report_url) = &config.report_url {
			let path = format!("{report_url}/flamegraph.svg");
			let mut file = std::fs::File::create(&path).unwrap();
			let mut options = Options::default();
			options.title = "rinha-de-backend".to_string();
			options.count_name = "samples".to_string();
			report
				.flamegraph_with_options(&mut file, &mut options)
				.unwrap();
		}
	}

	result
}
