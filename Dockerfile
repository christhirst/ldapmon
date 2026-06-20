# --- Build Stage ---
FROM rust:1.83-slim AS builder

# Install system dependencies needed for compiling
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /usr/src/ldapmon

# Copy source code
COPY . .

# Build the release binary
RUN cargo build --release

# --- Runtime Stage ---
FROM debian:bookworm-slim

# Install ca-certificates (crucial for LDAPS connections)
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy binary from builder stage
COPY --from=builder /usr/src/ldapmon/target/release/ldapmon /app/ldapmon

# Create a default volume or directory for the config file
VOLUME /app/config

# Set default env variable for tracing/logging
ENV RUST_LOG=ldapmon=info,info

# Expose REST API port
EXPOSE 8080

# Command to run the application, looking for config in the app directory
ENTRYPOINT ["/app/ldapmon", "/app/config.yaml"]
