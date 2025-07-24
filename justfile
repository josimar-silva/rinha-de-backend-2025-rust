test:
	just clean-containers
	cargo tarpaulin --out Lcov --all-features

format:
	cargo fmt --all

lint:
	cargo clippy --all-targets -- -D warnings

clean-containers:
	docker ps -aq | xargs -r docker rm -f
