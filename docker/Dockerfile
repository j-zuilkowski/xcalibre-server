FROM rust:1.77-slim AS builder
WORKDIR /app
COPY . .
RUN cargo build --release -p backend

FROM debian:bookworm-slim
WORKDIR /app
COPY --from=builder /app/target/release/backend /app/backend
EXPOSE 8083
USER 1000:1000
CMD ["/app/backend"]

