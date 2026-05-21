# syntax=docker/dockerfile:1.7

FROM rust:1.86-slim-bookworm AS builder
WORKDIR /build

RUN apt-get update && apt-get install -y --no-install-recommends \
        pkg-config \
    && rm -rf /var/lib/apt/lists/*

COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY migrations ./migrations
COPY tests ./tests

RUN cargo build --release --bin tempmail

FROM debian:bookworm-slim AS runtime
RUN apt-get update && apt-get install -y --no-install-recommends \
        ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd --gid 1000 tempmail \
    && useradd --uid 1000 --gid 1000 --shell /usr/sbin/nologin --create-home tempmail

WORKDIR /app
COPY --from=builder /build/target/release/tempmail /usr/local/bin/tempmail

USER 1000:1000
ENV TEMPMAIL_CONFIG=/etc/tempmail/config.toml \
    RUST_LOG=tempmail=info,tower_http=info,axum=info

EXPOSE 8080 1025
ENTRYPOINT ["/usr/local/bin/tempmail"]
