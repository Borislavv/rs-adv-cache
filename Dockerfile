### Multi-stage Dockerfile for advcache
### Builder stage: reproducible release build with dependency caching
FROM rust:1.82-slim AS builder

# Speed up cargo downloads and keep output readable in CI
ENV CARGO_TERM_COLOR=always \
    CARGO_REGISTRIES_CRATES_IO_PROTOCOL=sparse

# System deps for reqwest (native-tls) and general builds
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config \
    libssl-dev \
    ca-certificates \
 && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Cache dependency resolution
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo "fn main() {}" > src/main.rs
RUN cargo fetch --locked

# Copy full source and build
COPY . .
RUN cargo build --locked --release


### Runtime stage: small, non-root image
FROM debian:bookworm-slim AS runtime

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
 && rm -rf /var/lib/apt/lists/*

WORKDIR /app
RUN useradd -m appuser
USER appuser

COPY --from=builder /app/target/release/advcache /usr/local/bin/advcache

# Adjust if your service listens on a different port
EXPOSE 8080

ENV RUST_LOG=info

ENTRYPOINT ["/usr/local/bin/advcache"]

