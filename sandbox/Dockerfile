# syntax=docker/dockerfile:1

FROM rust:1.88-bookworm AS builder

WORKDIR /app

RUN apt-get update \
    && apt-get install -y --no-install-recommends pkg-config libssl-dev ca-certificates protobuf-compiler \
    && rm -rf /var/lib/apt/lists/*

COPY . .

ENV SQLX_OFFLINE=true
RUN cargo build --locked --release -p sandbox --bin sandbox

FROM debian:bookworm-slim

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates libssl3 \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY --from=builder /app/target/release/sandbox /usr/local/bin/sandbox
COPY sandbox/migrations /app/migrations

RUN mkdir -p /data

EXPOSE 8080

ENV PORT=8080 \
    DATABASE_URL=sqlite:/data/events.db \
    SQLX_OFFLINE=true \
    FASTEMBED_CACHE_PATH=/data/.fastembed_cache \
    RUST_LOG=info

CMD ["sandbox"]
