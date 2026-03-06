# Multi-stage Dockerfile: build with Rust, run minimal runtime image (distroless)
# Builder stage
FROM rust:latest AS builder

# Set working dir
WORKDIR /usr/src/chat

# Cache dependencies by copying manifest first
COPY Cargo.toml Cargo.lock ./
# Copy source
COPY src ./src

# Build release binary
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/* \
    && cargo build --release --bin chat_server

# Final stage: minimal runtime
FROM gcr.io/distroless/cc-debian12

# Copy binary and CA certs from builder
COPY --from=builder /usr/src/chat/target/release/chat_server /usr/local/bin/chat_server
COPY --from=builder /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/

EXPOSE 8080

HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
  CMD ["/usr/local/bin/chat_server", "--help"] || exit 1

ENTRYPOINT ["/usr/local/bin/chat_server"]
