# syntax=docker/dockerfile:1

FROM rust:1.97-slim-bookworm AS builder

WORKDIR /app
COPY . .
RUN cargo build --release -p website_backend

FROM debian:bookworm-slim AS runtime

RUN useradd --system --uid 10001 --no-create-home --shell /usr/sbin/nologin aimer

WORKDIR /app
COPY --from=builder /app/target/release/website_backend /usr/local/bin/website_backend
COPY --chown=10001:10001 website_backend/content/blogs/ /app/content/blogs/

ENV AIMER_BLOG_ADDRESS=0.0.0.0:3200 \
    AIMER_BLOG_DIR=/app/content/blogs

EXPOSE 3200
USER 10001
ENTRYPOINT ["/usr/local/bin/website_backend"]