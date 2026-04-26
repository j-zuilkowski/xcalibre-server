FROM rust:1.77-slim AS builder
ARG TARGETARCH
ARG TARGETVARIANT
WORKDIR /app

RUN set -eux; \
    targetarch="${TARGETARCH:-amd64}"; \
    apt-get update; \
    case "$targetarch" in \
        amd64) extra_packages="";; \
        arm64) extra_packages="gcc-aarch64-linux-gnu libc6-dev-arm64-cross";; \
        arm) extra_packages="gcc-arm-linux-gnueabihf libc6-dev-armhf-cross";; \
        *) echo "unsupported TARGETARCH: $targetarch" >&2; exit 1;; \
    esac; \
    apt-get install -y --no-install-recommends \
        pkg-config \
        libsqlite3-dev \
        binutils \
        $extra_packages; \
    rm -rf /var/lib/apt/lists/*

RUN set -eux; \
    case "${TARGETARCH:-amd64}" in \
        arm64) rustup target add aarch64-unknown-linux-gnu ;; \
        arm) \
            case "${TARGETVARIANT:-v7}" in \
                v7) rustup target add armv7-unknown-linux-gnueabihf ;; \
                *) echo "unsupported TARGETVARIANT for arm: ${TARGETVARIANT:-unset}" >&2; exit 1 ;; \
            esac ;; \
        amd64) : ;; \
        *) echo "unsupported TARGETARCH: ${TARGETARCH:-unset}" >&2; exit 1 ;; \
    esac

COPY Cargo.toml Cargo.lock ./
COPY backend/Cargo.toml backend/Cargo.toml
RUN mkdir -p backend/src \
    && printf "fn main() {}\n" > backend/src/main.rs \
    && cat <<'EOF' >/usr/local/bin/cargo-build-backend \
#!/bin/sh
set -eu
case "${TARGETARCH:-amd64}" in
    amd64)
        cargo build --release --locked -p backend
        ;;
    arm64)
        env \
            CARGO_BUILD_TARGET=aarch64-unknown-linux-gnu \
            CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc \
            cargo build --release --locked -p backend
        mkdir -p /app/target/release
        cp /app/target/aarch64-unknown-linux-gnu/release/backend /app/target/release/backend
        ;;
    arm)
        case "${TARGETVARIANT:-v7}" in
            v7)
                env \
                    CARGO_BUILD_TARGET=armv7-unknown-linux-gnueabihf \
                    CARGO_TARGET_ARMV7_UNKNOWN_LINUX_GNUEABIHF_LINKER=arm-linux-gnueabihf-gcc \
                    cargo build --release --locked -p backend
                mkdir -p /app/target/release
                cp /app/target/armv7-unknown-linux-gnueabihf/release/backend /app/target/release/backend
                ;;
            *)
                echo "unsupported TARGETVARIANT for arm: ${TARGETVARIANT:-unset}" >&2
                exit 1
                ;;
        esac
        ;;
    *)
        echo "unsupported TARGETARCH: ${TARGETARCH:-unset}" >&2
        exit 1
        ;;
esac
EOF
    && chmod +x /usr/local/bin/cargo-build-backend \
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

ENV RUST_LOG=warn
EXPOSE 8083
USER app
CMD ["/app/backend"]
