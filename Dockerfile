# Build stage
FROM rust:latest AS builder

WORKDIR /app

# Install build dependencies
RUN apt-get update && \
    apt-get install -y \
    cmake \
    build-essential \
    pkg-config \
    libclang-dev \
    clang \
    && rm -rf /var/lib/apt/lists/*

# Find and set LIBCLANG_PATH
RUN find /usr -name "libclang.so*" -exec dirname {} \; | head -n 1 > /tmp/libclang_path && \
    export LIBCLANG_PATH=$(cat /tmp/libclang_path) && \
    echo "LIBCLANG_PATH=$LIBCLANG_PATH" >> /etc/environment

# Copy only the dependency files first
COPY Cargo.toml Cargo.lock ./

# Create dummy source and bin directory structure
RUN mkdir -p src/bin/work_hours && \
    echo "fn main() {}" > src/main.rs && \
    echo "fn main() {}" > src/bin/get_calendar_token.rs && \
    echo "fn main() {}" > src/bin/work_hours/main.rs && \
    . /etc/environment && \
    cargo build --release && \
    cargo build --release --bin work_hours --features web-interface && \
    rm -rf src

# Now copy the actual source code
COPY src/ ./src/

# Build the actual applications
RUN . /etc/environment && \
    cargo build --release --bin mussubotti && \
    cargo build --release --bin work_hours --features web-interface

# Runtime stage
FROM gcr.io/distroless/cc-debian12

WORKDIR /app

# Copy the compiled binaries from builder
COPY --from=builder /app/target/release/mussubotti /app/mussubotti
COPY --from=builder /app/target/release/work_hours /app/work_hours

# Set the entrypoint
ENTRYPOINT ["/app/mussubotti"]

# Add health check
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
    CMD ["/bin/sh", "-c", "ps aux | grep mussubotti | grep -v grep"]
