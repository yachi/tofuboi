# Build stage
FROM rust:1.80 AS builder

WORKDIR /app

# Copy manifests
COPY Cargo.toml Cargo.lock ./

# Create a dummy main.rs to build dependencies
RUN mkdir src && \
    echo "fn main() {}" > src/main.rs && \
    cargo build --release

# Copy actual source code
COPY src ./src

# Build the application
# RUN cargo build --release
# CMD ["/app/target/release/tofuboi"]

CMD ["cargo", "run"] 
#  
# # Runtime stage
# FROM rust:1.80
# 
# WORKDIR /app
# 
# # Copy the binary from builder
# COPY --from=builder /app/target/release/tofuboi ./
# 
# CMD ["/app/tofuboi"]
