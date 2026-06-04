FROM rust:1.80-slim AS backend-builder
WORKDIR /build
RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*
COPY backend/ ./backend/
WORKDIR /build/backend
RUN cargo build --release

FROM node:20-slim AS frontend-builder
WORKDIR /build/frontend
COPY frontend/package*.json ./
RUN npm ci
COPY frontend/ ./
RUN npm run build

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates curl && rm -rf /var/lib/apt/lists/*
RUN useradd -r -s /bin/false voidtower
COPY --from=backend-builder /build/backend/target/release/voidtower /usr/local/bin/voidtower
COPY --from=frontend-builder /build/frontend/dist /usr/share/voidtower/frontend
RUN mkdir -p /etc/voidtower /var/lib/voidtower && \
    chown -R voidtower:voidtower /etc/voidtower /var/lib/voidtower && \
    chmod 700 /etc/voidtower
EXPOSE 8743 8745
USER voidtower
CMD ["/usr/local/bin/voidtower", "--bind", "0.0.0.0"]
