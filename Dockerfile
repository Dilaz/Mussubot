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
RUN mkdir -p src/bin && \
    echo "fn main() {}" > src/main.rs && \
    echo "fn main() {}" > src/bin/get_calendar_token.rs && \
    . /etc/environment && \
    cargo build --release && \
    rm -rf src

# Now copy the actual source code
COPY src/ ./src/

# Build the actual application
RUN . /etc/environment && \
    cargo build --release --bin mussubotti

# Runtime stage
FROM gcr.io/distroless/cc-debian12

WORKDIR /app

# Copy the compiled binary from builder
COPY --from=builder /app/target/release/mussubotti /app/mussubotti

# Set the entrypoint
ENTRYPOINT ["/app/mussubotti"]
