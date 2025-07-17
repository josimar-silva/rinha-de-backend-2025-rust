test:
	TESTCONTAINERS_COMMAND=keep cargo tarpaulin --out Lcov --all-features && just clean-containers

format:
	cargo fmt --all

lint:
	cargo clippy --all-targets -- -D warnings

clean-containers:
	docker rm -f $(docker ps -aq)
