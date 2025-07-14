test:
	TESTCONTAINERS_COMMAND=keep cargo tarpaulin --out Lcov --all-features && docker rm -f $(docker ps -aq)

format:
	cargo fmt --all

lint:
	cargo clippy --all-targets -- -D warnings

