# Build stage
FROM rust:1.83-slim AS builder

RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY . .
RUN cargo build --release

# Runtime stage
FROM debian:bookworm-slim

RUN apt-get update && \
    apt-get install -y --no-install-recommends ca-certificates && \
    rm -rf /var/lib/apt/lists/* && \
    useradd -r -s /bin/false datapace && \
    mkdir -p /data && chown datapace:datapace /data

COPY --from=builder /app/target/release/datapace-agent /usr/local/bin/datapace-agent
COPY --from=builder /app/config/agent.example.toml /app/agent.example.toml

USER datapace
WORKDIR /app

VOLUME /data

HEALTHCHECK --interval=30s --timeout=5s --start-period=5s --retries=3 \
    CMD wget --no-verbose --tries=1 --spider http://localhost:7080/api/health || exit 1

ENTRYPOINT ["datapace-agent"]
CMD ["--config", "/app/agent.toml"]

LABEL org.opencontainers.image.source="https://github.com/datapace-ai/datapace-agent"
LABEL org.opencontainers.image.description="Datapace Agent — PostgreSQL monitoring with local storage and web UI"
LABEL org.opencontainers.image.licenses="Apache-2.0"
