FROM lukemathwalker/cargo-chef:0.1.72-rust-1.88-slim-trixie AS chef
WORKDIR app

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder

COPY --from=planner /app/recipe.json recipe.json

ENV CARGO_NET_GIT_FETCH_WITH_CLI=true

RUN cargo chef cook --release --recipe-path recipe.json
COPY . .

RUN cargo build --release --locked --no-default-features

FROM debian:trixie-slim as runner

WORKDIR /app

# Install profiling tools
RUN apt-get update && apt-get install -y \
    linux-perf \
    procps \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/rinha-de-backend .

EXPOSE 9999

CMD ["./rinha-de-backend"]
