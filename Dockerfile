FROM rust:1.90 AS builder
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --release --bin watcher --bin slackbot

FROM ubuntu:24.04
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/watcher /usr/local/bin/
COPY --from=builder /app/target/release/slackbot /usr/local/bin/
WORKDIR /app