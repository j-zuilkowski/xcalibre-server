FROM rust:1.94-slim AS builder
WORKDIR /app

RUN set -eux; \
    apt-get update; \
    apt-get install -y --no-install-recommends \
        pkg-config \
        libsqlite3-dev \
        libssl-dev \
        binutils \
        g++ \
        make; \
    rm -rf /var/lib/apt/lists/*

COPY Cargo.toml Cargo.lock ./
COPY backend/Cargo.toml backend/Cargo.toml
COPY backend/benches backend/benches
COPY xs-migrate/Cargo.toml xs-migrate/Cargo.toml
COPY xs-mcp/Cargo.toml xs-mcp/Cargo.toml
COPY vendor/csv vendor/csv
COPY vendor/swagger-ui-v5.17.14.zip vendor/swagger-ui-v5.17.14.zip
COPY xs-migrate/src xs-migrate/src
COPY xs-mcp/src xs-mcp/src
ENV SWAGGER_UI_DOWNLOAD_URL=file:///app/vendor/swagger-ui-v5.17.14.zip
RUN mkdir -p backend/src \
    && printf "fn main() {}\n" > backend/src/main.rs \
    && cat <<'EOF' >/usr/local/bin/cargo-build-backend
#!/bin/sh
set -eu
cargo build --release --locked -p backend
EOF

RUN chmod +x /usr/local/bin/cargo-build-backend \
    && /usr/local/bin/cargo-build-backend \
    && rm -rf backend/src

COPY backend backend
# Placeholder for future frontend asset build stage.
RUN /usr/local/bin/cargo-build-backend \
    && strip /app/target/release/backend

FROM node:20-slim AS web-builder
WORKDIR /app

RUN npm install -g pnpm

COPY package.json pnpm-lock.yaml pnpm-workspace.yaml ./
COPY apps/web apps/web
COPY packages/shared packages/shared

RUN pnpm install --frozen-lockfile \
    && pnpm --filter @xs/web build

FROM debian:bookworm-slim AS runtime
WORKDIR /app

RUN groupadd --gid 10001 app \
    && useradd --uid 10001 --gid app --home-dir /app --shell /usr/sbin/nologin --create-home app \
    && mkdir -p /app/storage \
    && chown -R app:app /app
RUN apt-get update \
    && apt-get install -y --no-install-recommends libsqlite3-0 libssl3 \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/backend /app/backend
COPY --from=web-builder /app/apps/web/dist /app/web

ENV WEB_DIST_DIR=/app/web

ENV RUST_LOG=warn
EXPOSE 8083
USER app
CMD ["/app/backend"]
