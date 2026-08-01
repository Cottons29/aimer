# syntax=docker/dockerfile:1
#
# Image for the blog API. The build context is the repository root because
# `website_backend` is a Cargo workspace member.
#
# Runtime settings come from the environment (`SERVER_IP`, `SERVER_PORT`,
# `SERVER_CORS`); the values below are the defaults for a plain `docker run`
# and are overridden by the container platform — on Cloudflare by the `[vars]`
# table in `wrangler.toml`.

FROM rust:1.97-slim-bookworm AS builder

WORKDIR /app
COPY . .
RUN cargo build --release -p website_backend --bin website_backend

FROM debian:bookworm-slim AS runtime

RUN useradd --system --uid 10001 --no-create-home --shell /usr/sbin/nologin aimer

WORKDIR /app
COPY --from=builder /app/target/release/website_backend /usr/local/bin/website_backend
COPY --chown=10001:10001 website_backend/content/blogs/ /app/content/blogs/

ENV AIMER_BLOG_DIR=/app/content/blogs
ENV SERVER_IP=0.0.0.0
ENV SERVER_PORT=3200
ENV SERVER_CORS=https://aimer.cottonsofficial.com,http://localhost:3000

EXPOSE 3200
USER 10001
ENTRYPOINT ["/usr/local/bin/website_backend"]