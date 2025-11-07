# Build stage - compile both binaries
FROM rust:1.90 AS builder
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --release --bin watcher --bin slackbot

# Base runtime stage with minimal dependencies
# Note: Ubuntu 24.04 is required for GLIBC 2.38 support
FROM ubuntu:24.04 AS runtime-base
RUN apt-get update && \
    apt-get install -y ca-certificates && \
    rm -rf /var/lib/apt/lists/*
WORKDIR /app

# Watcher service stage
FROM runtime-base AS watcher
COPY --from=builder /app/target/release/watcher /usr/local/bin/watcher
CMD ["watcher"]

# Slackbot service stage
FROM runtime-base AS slackbot
COPY --from=builder /app/target/release/slackbot /usr/local/bin/slackbot
CMD ["slackbot"]
