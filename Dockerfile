ARG BUILD_VARIANT=prod

FROM lukemathwalker/cargo-chef:0.1.72-rust-1.88-slim-trixie AS chef
WORKDIR /app

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS prod_builder

COPY --from=planner /app/recipe.json recipe.json

ENV CARGO_NET_GIT_FETCH_WITH_CLI=true

RUN cargo chef cook --release --recipe-path recipe.json
COPY . .

ENV RUSTFLAGS="-C target-cpu=native"

RUN cargo build --release --locked --no-default-features

FROM prod_builder AS perf_builder

ENV RUSTFLAGS="-C target-cpu=native"

RUN cargo build --release --locked --features perf
COPY . .

FROM ${BUILD_VARIANT}_builder AS builder

FROM debian:trixie-slim AS runner

WORKDIR /app

COPY --from=builder /app/target/release/rinha-de-backend .

EXPOSE 9999

CMD ["./rinha-de-backend"]
