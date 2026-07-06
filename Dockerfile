# syntax=docker/dockerfile:1
# Multi-stage build:
#   1. cargo-chef caches the dependency compilation layer — rebuilding after a
#      source-only change never recompiles the (large) dependency tree.
#   2. Static musl release build (rust:alpine's default target is musl with
#      +crt-static, on both x86_64 and aarch64).
#   3. Final image is distroless/static, non-root, ~a dozen MB + binary.

# ---- chef: rust + musl toolchain + cargo-chef ----
FROM lukemathwalker/cargo-chef:latest-rust-alpine AS chef
WORKDIR /app
# aws-lc-rs (crypto backend of the AWS SDK's rustls) compiles C at build time
RUN apk add --no-cache musl-dev cmake make perl clang linux-headers

# ---- planner: compute the dependency recipe ----
FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

# ---- builder ----
FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json
# dependency layer — cached until Cargo.toml/Cargo.lock change
RUN cargo chef cook --release --recipe-path recipe.json
COPY . .
# compile-time-checked SQL reads the committed .sqlx cache — no database needed
ENV SQLX_OFFLINE=true
RUN cargo build --release --bin citra-petcare \
    && cp target/release/citra-petcare /citra-petcare

# ---- runtime: distroless static, non-root ----
FROM gcr.io/distroless/static-debian12:nonroot
COPY --from=builder /citra-petcare /usr/local/bin/citra-petcare
USER nonroot
EXPOSE 8080
ENTRYPOINT ["/usr/local/bin/citra-petcare"]
CMD ["serve"]
