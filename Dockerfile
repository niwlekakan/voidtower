FROM rust:1.86-slim AS backend-builder
WORKDIR /build
RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*
COPY backend/ ./backend/
WORKDIR /build/backend
RUN cargo build --release

FROM node:22-slim AS frontend-builder
WORKDIR /build/frontend
COPY frontend/package*.json ./
RUN npm ci
COPY frontend/ ./
RUN npm run build

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y \
        ca-certificates curl openssl \
        nginx supervisor \
    && rm -rf /var/lib/apt/lists/*
RUN useradd -r -s /bin/false voidtower
COPY --from=backend-builder /build/backend/target/release/voidtower /usr/local/bin/voidtower
COPY --from=frontend-builder /build/frontend/dist /usr/share/voidtower/frontend
COPY nginx/voidtower.conf /etc/nginx/conf.d/voidtower.conf
COPY docker/supervisord.conf /etc/supervisor/conf.d/voidtower.conf
COPY docker/entrypoint.sh /entrypoint.sh
RUN rm -f /etc/nginx/sites-enabled/default && \
    mkdir -p /etc/voidtower/tls /var/lib/voidtower && \
    chown -R voidtower:voidtower /var/lib/voidtower && \
    chmod 700 /etc/voidtower && \
    chmod +x /entrypoint.sh
EXPOSE 80 443 8745
ENTRYPOINT ["/entrypoint.sh"]
