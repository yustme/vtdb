# Build stage - use bookworm-based Rust image for GLIBC compatibility
FROM rust:bookworm as builder

WORKDIR /app

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Copy manifest files
COPY Cargo.toml Cargo.lock ./

# Copy source code
COPY src ./src

# Build the application in release mode
RUN cargo build --release

# Runtime stage - use same Debian version as builder
FROM debian:bookworm-slim

WORKDIR /app

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    curl \
    && rm -rf /var/lib/apt/lists/*

# Copy the binary from builder stage
COPY --from=builder /app/target/release/vtdb /app/vtdb

# Copy web directory for static files
COPY web ./web

# Create data directory for persistence
RUN mkdir -p /app/data

# Expose the port
EXPOSE 8080

# Run the application
CMD ["./vtdb"]

