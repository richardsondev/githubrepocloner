# Multi-stage build for githubrepocloner
FROM rust:1.94 AS builder

WORKDIR /app

# Copy only what's needed for the build
COPY Cargo.toml Cargo.lock build.rs ./
COPY src/ src/
COPY tests/ tests/
COPY img/ img/

# Run tests first
RUN cargo test --release

# Build the release binary
RUN cargo build --release

# Runtime stage - minimal image
FROM debian:trixie-slim

# Install git (needed for git clone mode) and ca-certificates (for HTTPS)
RUN apt-get update && \
    apt-get install -y --no-install-recommends git ca-certificates && \
    rm -rf /var/lib/apt/lists/*

# Copy the binary from builder
COPY --from=builder /app/target/release/githubrepocloner /usr/local/bin/githubrepocloner

# Create a directory for cloned repos
WORKDIR /repos

# Set the binary as entrypoint
ENTRYPOINT ["githubrepocloner"]

# Default help output if no args provided
CMD ["--help"]
