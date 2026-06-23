# --- Build Stage ---
FROM rust:1.96-slim AS builder

RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /usr/src/ldapmon

# Cache dependencies as a separate layer.
# This layer is only invalidated when Cargo.toml or Cargo.lock change,
# not on every source edit.
COPY Cargo.toml Cargo.lock ./
RUN cargo fetch

# Copy source and build
COPY src ./src
RUN cargo build --release

# --- Runtime Stage ---
# gcr.io/distroless/cc-debian12 provides libc + libgcc (needed by Rust binaries)
# and ships Mozilla's CA bundle — no shell or package manager included.
FROM gcr.io/distroless/cc-debian12

WORKDIR /app

COPY --from=builder /usr/src/ldapmon/target/release/ldapmon /app/ldapmon

VOLUME /app/config

ENV RUST_LOG=ldapmon=info,info

EXPOSE 8080

ENTRYPOINT ["/app/ldapmon"]
