# syntax=docker/dockerfile:1

FROM rust:1.91-bookworm AS builder

RUN apt-get update \
    && apt-get install -y --no-install-recommends pkg-config \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY core core
COPY cli cli
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/src/target \
    cargo build --release -p sculk-cli --features insecure-relay-tls \
    && cp target/release/sculk /sculk

FROM debian:bookworm-slim

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        ca-certificates \
        curl \
        iproute2 \
        iptables \
        netcat-openbsd \
        openssl \
        socat \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /sculk /usr/local/bin/sculk
