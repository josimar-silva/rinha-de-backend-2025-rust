FROM rust:1.88-slim-bookworm as builder

WORKDIR /app

RUN apt-get update && apt-get install -y --no-install-recommends libssl-dev pkg-config && rm -rf /var/lib/apt/lists/*

COPY ./src ./src
COPY ./Cargo.toml ./Cargo.toml
COPY ./Cargo.lock ./Cargo.lock

RUN cargo build --release --locked --target x86_64-unknown-linux-gnu

FROM debian:bookworm-slim

WORKDIR /app

COPY --from=builder /app/target/x86_64-unknown-linux-gnu/release/backend .

EXPOSE 9999

CMD ["./backend"]
