FROM rust:latest AS backend-builder
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

FROM ubuntu:24.04
RUN apt-get update && apt-get install -y \
        ca-certificates curl openssl gnupg lsb-release \
        nginx supervisor \
    && install -m 0755 -d /etc/apt/keyrings \
    && curl -fsSL https://download.docker.com/linux/ubuntu/gpg \
       | gpg --dearmor -o /etc/apt/keyrings/docker.gpg \
    && echo "deb [arch=$(dpkg --print-architecture) signed-by=/etc/apt/keyrings/docker.gpg] \
       https://download.docker.com/linux/ubuntu $(lsb_release -cs) stable" \
       > /etc/apt/sources.list.d/docker.list \
    && apt-get update \
    && apt-get install -y docker-ce-cli docker-compose-plugin \
    && rm -rf /var/lib/apt/lists/*
COPY --from=backend-builder /build/backend/target/release/voidtower /usr/local/bin/voidtower
COPY --from=frontend-builder /build/frontend/dist /usr/share/voidtower/frontend
COPY app-vault/apps /usr/share/voidtower/apps
COPY nginx/voidtower.conf /etc/nginx/conf.d/voidtower.conf
COPY docker/supervisord.conf /etc/supervisor/conf.d/voidtower.conf
COPY docker/entrypoint.sh /entrypoint.sh
RUN rm -f /etc/nginx/sites-enabled/default && \
    mkdir -p /etc/voidtower/tls /var/lib/voidtower && \
    chmod +x /entrypoint.sh
EXPOSE 80 443 8745
ENTRYPOINT ["/entrypoint.sh"]
