# Build stage - compile both binaries
FROM rust:1.90 AS builder
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --release --bin slackbot

# Base runtime stage with minimal dependencies
# Note: Ubuntu 24.04 is required for GLIBC 2.38 support
FROM ubuntu:24.04 AS runtime-base
RUN apt-get update && \
    apt-get install -y ca-certificates && \
    rm -rf /var/lib/apt/lists/*
WORKDIR /app


# Slackbot service stage
FROM runtime-base AS slackbot
COPY --from=builder /app/target/release/slackbot /usr/local/bin/slackbot
CMD ["slackbot"]
