# syntax=docker/dockerfile:1

# --------------------------------------------------------------------------- #
# Build stage: compile the Rust -> WASM site with Trunk.
#
# NIGHTLY is required, not a preference: the workspace `.cargo/config.toml` sets
#   [unstable] build-std = ["panic_abort", "std"]
#   [target.wasm32-unknown-unknown] target-feature = +atomics,+bulk-memory
# so std has to be recompiled with atomics, which only nightly + the `rust-src`
# component can do.
# --------------------------------------------------------------------------- #
FROM rustlang/rust:nightly-slim AS builder

# ca-certificates: Trunk downloads pinned wasm-bindgen / wasm-opt over TLS.
# pkg-config + libssl-dev: host build of `trunk`/its deps may link openssl-sys.
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates pkg-config libssl-dev \
 && rm -rf /var/lib/apt/lists/*

RUN rustup component add rust-src \
 && rustup target add wasm32-unknown-unknown \
 && cargo install trunk --locked

WORKDIR /app
# The website crate depends on the root `aimer` crate and every crate in the
# workspace, so the whole repo is the build context (see .dockerignore).
COPY . .

# index.html points at ../../Cargo.toml and Trunk writes the bundle to ./dist.
# --all-features on wasm-opt is already wired via index.html's data-* attrs so
# validation matches the +atomics/+bulk-memory wasm rustc emits.
WORKDIR /app/website/builds/web
RUN trunk build --release

# --------------------------------------------------------------------------- #
# Runtime stage: serve the static bundle.
# --------------------------------------------------------------------------- #
FROM nginx:alpine AS runtime
COPY website/builds/web/nginx.conf /etc/nginx/conf.d/default.conf
COPY --from=builder /app/website/builds/web/dist /usr/share/nginx/html
EXPOSE 80
