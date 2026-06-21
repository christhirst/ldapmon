# --- Build Stage ---
FROM rust:1.96-slim AS builder

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
# gcr.io/distroless/cc-debian12 provides libc + libgcc (needed by Rust binaries)
# and ships Mozilla's CA bundle — no shell or package manager included.
FROM gcr.io/distroless/cc-debian12

WORKDIR /app

# Copy binary from builder stage
COPY --from=builder /usr/src/ldapmon/target/release/ldapmon /app/ldapmon

# Create a default volume or directory for the config file
VOLUME /app/config

# Set default env variable for tracing/logging
ENV RUST_LOG=ldapmon=info,info

# Expose REST API port
EXPOSE 8080

# Command to run the application (JSON array required — distroless has no shell)
ENTRYPOINT ["/app/ldapmon"]
