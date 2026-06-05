#!/bin/sh
set -e

# Fix ownership of bind-mounted volumes before dropping to voidtower user.
# On TrueNAS (and other hosts), mounted dirs are owned by root — the voidtower
# process runs as the voidtower user (supervisord user=voidtower) so it can't
# write unless we chown here while still running as root.
mkdir -p /var/lib/voidtower /etc/voidtower
chown -R voidtower:voidtower /var/lib/voidtower /etc/voidtower

TLS_DIR=/etc/voidtower/tls
NGINX_CONF=/etc/nginx/conf.d/voidtower.conf
BACKEND=http://127.0.0.1:8743

# Try to generate a self-signed cert on first run
HAS_TLS=0
if [ ! -f "$TLS_DIR/cert.pem" ]; then
    mkdir -p "$TLS_DIR"
    if openssl req -x509 -nodes -days 3650 \
        -newkey rsa:2048 \
        -keyout "$TLS_DIR/key.pem" \
        -out "$TLS_DIR/cert.pem" \
        -subj "/CN=voidtower" 2>&1; then
        chmod 600 "$TLS_DIR/key.pem"
        HAS_TLS=1
        echo "[entrypoint] TLS self-signed cert generated"
    else
        echo "[entrypoint] WARNING: TLS cert generation failed — running HTTP-only on port 80"
        rm -f "$TLS_DIR/key.pem" "$TLS_DIR/cert.pem"
    fi
else
    HAS_TLS=1
fi

# Write nginx config based on TLS availability
_proxy_headers='
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_read_timeout 3600;
        proxy_send_timeout 3600;
        client_max_body_size 100m;'

cat > "$NGINX_CONF" <<NGINX
server {
    listen 80;
    server_name _;
    location / {
        proxy_pass $BACKEND;$_proxy_headers
        proxy_set_header X-Forwarded-Proto http;
    }
}
NGINX

if [ "$HAS_TLS" = "1" ]; then
    cat >> "$NGINX_CONF" <<NGINX

server {
    listen 443 ssl;
    server_name _;
    ssl_certificate     $TLS_DIR/cert.pem;
    ssl_certificate_key $TLS_DIR/key.pem;
    ssl_protocols       TLSv1.2 TLSv1.3;
    ssl_ciphers         HIGH:!aNULL:!MD5;
    ssl_session_cache   shared:SSL:10m;
    ssl_session_timeout 10m;
    location / {
        proxy_pass $BACKEND;$_proxy_headers
        proxy_set_header X-Forwarded-Proto https;
    }
}
NGINX
fi

exec /usr/bin/supervisord -c /etc/supervisor/conf.d/voidtower.conf
