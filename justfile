test:
	CARGO_TARGET_DIR=./target RUSTFLAGS="-C target-cpu=native" cargo tarpaulin --out Lcov --all-features

format:
	CARGO_TARGET_DIR=./target RUSTFLAGS="-C target-cpu=native" cargo fmt --all

lint:
	CARGO_TARGET_DIR=./target RUSTFLAGS="-C target-cpu=native" cargo clippy --all-targets -- -D warnings

clean-containers:
	docker rm -f $(docker ps -aq)
