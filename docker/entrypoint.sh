#!/bin/sh
set -e

TLS_DIR=/etc/voidtower/tls

# Generate a self-signed cert on first run if no cert is present
if [ ! -f "$TLS_DIR/cert.pem" ]; then
    mkdir -p "$TLS_DIR"
    openssl req -x509 -nodes -days 3650 \
        -newkey rsa:2048 \
        -keyout "$TLS_DIR/key.pem" \
        -out "$TLS_DIR/cert.pem" \
        -subj "/CN=voidtower" \
        -quiet 2>/dev/null || true
    chmod 600 "$TLS_DIR/key.pem"
fi

exec /usr/bin/supervisord -c /etc/supervisor/conf.d/voidtower.conf
