# Build stage
FROM rust:1.83-alpine AS builder

# Install build dependencies
RUN apk add --no-cache musl-dev openssl-dev openssl-libs-static pkgconfig

WORKDIR /app

# Copy manifests first for better caching
COPY Cargo.toml Cargo.lock* ./

# Create dummy main.rs to cache dependencies
RUN mkdir src && \
    echo "fn main() {}" > src/main.rs && \
    echo "pub fn lib() {}" > src/lib.rs

# Build dependencies only
RUN cargo build --release && \
    rm -rf src

# Copy actual source code
COPY src ./src

# Touch main.rs to trigger rebuild
RUN touch src/main.rs src/lib.rs

# Build the actual application
RUN cargo build --release

# Runtime stage
FROM alpine:3.21

# Install CA certificates for TLS
RUN apk add --no-cache ca-certificates tzdata

# Create non-root user
RUN addgroup -g 1000 datapace && \
    adduser -u 1000 -G datapace -s /bin/sh -D datapace

WORKDIR /app

# Copy binary from builder
COPY --from=builder /app/target/release/datapace-agent /usr/local/bin/datapace-agent

# Copy example config
COPY configs/agent.example.yaml /app/agent.example.yaml

# Use non-root user
USER datapace

# Health check
HEALTHCHECK --interval=30s --timeout=5s --start-period=5s --retries=3 \
    CMD wget --no-verbose --tries=1 --spider http://localhost:8080/health || exit 1

# Default command
ENTRYPOINT ["datapace-agent"]
CMD []

# Labels
LABEL org.opencontainers.image.source="https://github.com/datapace-ai/datapace-agent"
LABEL org.opencontainers.image.description="Datapace Agent - Database metrics collector"
LABEL org.opencontainers.image.licenses="Apache-2.0"
