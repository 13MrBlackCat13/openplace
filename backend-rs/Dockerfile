# openplace backend (Rust) — multi-stage build
FROM rust:1-slim AS builder
WORKDIR /build
RUN apt-get update && apt-get install -y pkg-config && rm -rf /var/lib/apt/lists/*
COPY Cargo.toml ./
COPY migrations ./migrations
# Dummy main to cache dependencies
RUN mkdir -p src/bin && echo "fn main() {}" > src/main.rs && echo "fn main() {}" > src/bin/loadgen.rs
RUN cargo build --release 2>/dev/null || true
COPY src ./src
RUN touch src/main.rs src/bin/loadgen.rs && cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /build/target/release/openplace-backend /usr/local/bin/openplace-backend
WORKDIR /app
EXPOSE 3000
CMD ["openplace-backend", "serve"]
