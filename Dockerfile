# Build stage
FROM rust:latest AS builder

WORKDIR /app

# Create a dummy project with the same dependencies
COPY Cargo.toml Cargo.lock ./

# Create dummy source to build dependencies
RUN mkdir -p src && \
    echo "fn main() {}" > src/main.rs && \
    cargo build --release && \
    rm -rf src

# Now copy the actual source code
COPY src/ ./src/

# Build the actual application
RUN cargo build --release

# Runtime stage
FROM gcr.io/distroless/cc-debian12

WORKDIR /app

# Copy the compiled binary from builder
COPY --from=builder /app/target/release/mussubot /app/mussubot

# Set the entrypoint
ENTRYPOINT ["/app/mussubot"]
