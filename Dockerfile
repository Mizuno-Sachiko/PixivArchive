FROM node:22.23.1-bookworm-slim AS frontend-builder

WORKDIR /src
RUN corepack enable && corepack prepare pnpm@11.8.0 --activate
COPY frontend/package.json frontend/pnpm-lock.yaml frontend/
RUN pnpm --dir frontend install --frozen-lockfile
COPY frontend frontend
RUN pnpm --dir frontend build

FROM rust:1.96-bookworm AS rust-builder

WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY .sqlx .sqlx
COPY apps apps
COPY crates crates
COPY migrations migrations
ENV SQLX_OFFLINE=true
RUN cargo build --locked --release \
    -p pixivarchive-web \
    -p pixivarchive-worker \
    -p pixivarchive-admin

FROM debian:bookworm-slim

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates libvips-tools \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /opt/pixivarchive
COPY LICENSE LICENSE
COPY --from=rust-builder /src/target/release/pixivarchive-web /usr/local/bin/
COPY --from=rust-builder /src/target/release/pixivarchive-worker /usr/local/bin/
COPY --from=rust-builder /src/target/release/pixivarchive-admin /usr/local/bin/
COPY --from=frontend-builder /src/frontend/build frontend

ENV PIXIVARCHIVE_STATIC_ROOT=/opt/pixivarchive/frontend
CMD ["pixivarchive-web"]
