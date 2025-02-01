# Build stage
FROM jonoh/sccache-rust AS base
RUN cargo install --locked cargo-chef
ENV RUSTC_WRAPPER=sccache SCCACHE_DIR=/sccache
 
FROM base AS planner
WORKDIR /app
COPY --link src src
COPY --link Cargo.* .
RUN cargo chef prepare --recipe-path recipe.json

FROM base AS builder
WORKDIR /app
COPY --from=planner /app/recipe.json recipe.json
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=$SCCACHE_DIR,sharing=locked \
    cargo chef cook --release --recipe-path recipe.json
COPY --link src src
COPY --link Cargo.* .
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=$SCCACHE_DIR,sharing=locked \
    cargo build --release

# Runtime stage
FROM rust:slim

# Copy the binary from builder
COPY --from=builder /app/target/release/tofuboi /app

CMD ["/app"]
