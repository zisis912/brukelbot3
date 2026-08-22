FROM lukemathwalker/cargo-chef:latest-rust-1 AS chef
WORKDIR /app

# Install sccache
RUN curl -L https://github.com/mozilla/sccache/releases/download/v0.17.0/sccache-v0.17.0-x86_64-unknown-linux-musl.tar.gz \
    | tar xz --strip-components=1 -C /usr/local/bin --wildcards '*/sccache'

ENV RUSTC_WRAPPER=sccache
ENV SCCACHE_DIR=/sccache

FROM chef AS planner
# COPY ./Cargo.toml ./Cargo.lock ./
# COPY ./src ./src
# COPY ./data ./data
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder 
COPY --from=planner /app/recipe.json recipe.json
# Build dependencies with sccache
RUN --mount=type=cache,target=/sccache \
    cargo chef cook --release --recipe-path recipe.json

# Build application
COPY . .
RUN --mount=type=cache,target=/sccache \
    cargo build --release && sccache --show-stats

# We do not need the Rust toolchain to run the binary
FROM debian:trixie-slim AS runtime
WORKDIR /app
COPY --from=builder /app/target/release/brukelbot3 /usr/local/bin
ENTRYPOINT ["/usr/local/bin/brukelbot3"]
