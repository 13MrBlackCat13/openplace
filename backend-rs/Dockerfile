# openplace backend (Rust) — multi-stage build
# Base images are digest-pinned; bump tag and digest together to update.
FROM rust:1-slim@sha256:2752b332db73fdbb7dc576f06c82ed1f312005784ef913d7e04a28f5f55dc581 AS builder
WORKDIR /build
RUN apt-get update && apt-get install -y pkg-config && rm -rf /var/lib/apt/lists/*
COPY Cargo.toml Cargo.lock ./
COPY migrations ./migrations
# Dummy main to cache dependencies
RUN mkdir -p src/bin && echo "fn main() {}" > src/main.rs && echo "fn main() {}" > src/bin/loadgen.rs
RUN cargo build --release --locked 2>/dev/null || true
COPY src ./src
RUN touch src/main.rs src/bin/loadgen.rs && cargo build --release --locked

FROM debian:bookworm-slim@sha256:7c7b2c966bc9ee8cedfeef67e0e279108992c77681fa595db4a9d65c06ccc587
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /build/target/release/openplace-backend /usr/local/bin/openplace-backend
WORKDIR /app
EXPOSE 3000
CMD ["openplace-backend", "serve"]
