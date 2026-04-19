FROM rust:1.77-slim AS builder
WORKDIR /app

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        pkg-config \
        libsqlite3-dev \
        binutils \
    && rm -rf /var/lib/apt/lists/*

COPY Cargo.toml Cargo.lock ./
COPY backend/Cargo.toml backend/Cargo.toml
RUN mkdir -p backend/src \
    && printf "fn main() {}\n" > backend/src/main.rs \
    && cargo build --release --locked -p backend \
    && rm -rf backend/src

COPY backend backend
# Placeholder for future frontend asset build stage.
RUN cargo build --release --locked -p backend \
    && strip /app/target/release/backend

FROM node:20-slim AS web-builder
WORKDIR /app

RUN npm install -g pnpm

COPY package.json pnpm-lock.yaml pnpm-workspace.yaml ./
COPY apps/web apps/web
COPY packages/shared packages/shared

RUN pnpm install --frozen-lockfile \
    && pnpm --filter web build

FROM debian:bookworm-slim AS runtime
WORKDIR /app

RUN groupadd --gid 10001 app \
    && useradd --uid 10001 --gid app --home-dir /app --shell /usr/sbin/nologin --create-home app \
    && mkdir -p /app/storage \
    && chown -R app:app /app
RUN apt-get update \
    && apt-get install -y --no-install-recommends libsqlite3-0 \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/backend /app/backend
COPY --from=web-builder /app/apps/web/dist /app/web

ENV WEB_DIST_DIR=/app/web

ENV RUST_LOG=info
EXPOSE 8083
USER app
CMD ["/app/backend"]
