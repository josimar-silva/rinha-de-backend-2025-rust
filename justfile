test:
	cargo tarpaulin --out Lcov --all-features

format:
	cargo fmt --all

lint:
	cargo clippy --all-targets -- -D warnings

clean-containers:
	docker rm -f $(docker ps -aq)
