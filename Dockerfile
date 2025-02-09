# Build stage: base with sccache and cargo-chef installed
FROM jonoh/sccache-rust:1.84.0 AS base
# Install cargo-chef for dependency caching, and set environment variables
ARG CARGO_CHEF_VERSION=0.1.71
RUN cargo install --locked cargo-chef --version ${CARGO_CHEF_VERSION}
ENV RUSTC_WRAPPER=sccache \
    SCCACHE_DIR=/sccache

# Planner stage: prepare the recipe used for caching dependencies
FROM base AS planner
WORKDIR /app
# Use BuildKit's --link to optimize copy performance (ensure BuildKit is enabled)
COPY --link Cargo.* .
COPY --link src/ src/
RUN cargo chef prepare --recipe-path recipe.json

# Builder stage: cook dependencies and build the binary using cached layers
FROM base AS builder
WORKDIR /app
COPY --from=planner /app/recipe.json recipe.json
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=${SCCACHE_DIR},sharing=locked \
    cargo chef cook --release --recipe-path recipe.json

COPY --link Cargo.* .
COPY --link src/ src/
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=${SCCACHE_DIR},sharing=locked \
    cargo build --release

# Runtime stage: use a minimal image with non-root user
FROM debian:bookworm-slim AS runtime
RUN apt-get update && \
    apt-get install -y --no-install-recommends openssl ca-certificates && \
    rm -rf /var/lib/apt/lists/*

WORKDIR /app
# Create a non-root user for running the app
RUN useradd -m appuser

# Copy the built binary from the builder stage; ensure proper permissions in one layer
COPY --from=builder /app/target/release/tofuboi ./tofuboi
RUN chown appuser:appuser ./tofuboi

USER appuser
CMD ["/app/tofuboi"]
