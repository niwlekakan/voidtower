# VoidTower Networking Reference

Complete reference for how VoidTower containers communicate with each other, how
inbound HTTP traffic is routed, and how to swap out any piece of the networking
stack.

---

## 1. Overview

VoidTower uses a two-layer Docker networking model:

- **vt-proxy** — an external Docker bridge network that every app container joins.
  Handles east-west (container-to-container) traffic. No host ports needed for
  inter-service communication.
- **nginx-proxy** — a reverse proxy container (`nginxproxy/nginx-proxy`) that
  handles north-south (inbound HTTP) traffic. Watches the Docker socket and
  auto-generates nginx vhosts from `VIRTUAL_HOST` env vars set on each container.

The practical result: containers talk to each other by service name over vt-proxy,
and users reach apps through `http://appname.local:8080` (or a DNS name that
resolves to the host) routed through nginx-proxy. No app needs to expose a host
port just to communicate with another app.

---

## 2. The vt-proxy Network

### What it is

`vt-proxy` is a standard Docker bridge network created once at install time and
marked `external: true` in every compose YAML. Docker Compose does not create or
destroy it — it is a shared, persistent network that outlives any individual app's
deployment lifecycle.

### Creating it

```bash
docker network create vt-proxy
```

VoidTower's installer runs this automatically. If you ever need to recreate it:

```bash
docker network rm vt-proxy
docker network create vt-proxy
```

All running containers that reference vt-proxy will lose connectivity until they
are restarted, so do this during maintenance only.

### Why `external: true`

Without `external: true`, Docker Compose would try to create a project-scoped
network named `<project>_vt-proxy` instead of joining the shared one. Every app
would be on its own isolated network and containers could not reach each other.
The `external: true` flag tells Compose "this network already exists outside my
project — connect to it as-is."

### Container DNS

Within vt-proxy, containers resolve each other by the service name and any
aliases declared under the service's `networks.vt-proxy.aliases` key. For
example, pihole declares:

```yaml
networks:
  vt-proxy:
    aliases:
      - pihole
```

Any other container on vt-proxy can reach pihole at `http://pihole:80`. The alias
is explicit and stable regardless of container ID or IP churn.

### The top-level `networks:` declaration requirement

Every YAML that references `vt-proxy` in a service's `networks:` block **must**
also have a top-level declaration inside `compose:`:

```yaml
compose:
  services:
    myapp:
      networks:
        vt-proxy:
          aliases:
            - myapp
  networks:
    vt-proxy:
      external: true   # <-- this is required
```

Without the top-level declaration, `docker compose up` fails:

```
network vt-proxy declared as external, but could not be found
```

This is the bug fixed in the commit that introduced this document: 31 app-vault
YAMLs were missing the top-level declaration.

### VoidTower's backend safety net

Even if a YAML is missing the declaration, VoidTower's backend injects it at
deploy time via `inject_external_networks()` in
`backend/src/api/apps.rs` (line 184). It scans every service's `networks` block,
and if any reference `vt-proxy`, it inserts the top-level
`networks: { vt-proxy: { external: true } }` before calling `docker compose up`.
The YAML fixes in this repo mean the function is a safety net, not a crutch.

---

## 3. nginx-proxy Routing

### How it works

`nginxproxy/nginx-proxy` mounts the Docker socket read-only and watches for
container start/stop events. When a container starts with `VIRTUAL_HOST` set, it
generates an nginx server block for that hostname pointing to the container's IP
on vt-proxy. When the container stops, the block is removed.

### Port layout

| Host port | Container port | Protocol |
|-----------|---------------|----------|
| 8080      | 80            | HTTP     |
| 8443      | 443           | HTTPS    |

Access any app at `http://<hostname>:8080` or `https://<hostname>:8443`.

### Reaching an app

The simplest approach is `/etc/hosts` on your client machine:

```
192.168.1.x  pihole.local gitea.local jellyfin.local nextcloud.local
```

Or use Pi-hole or AdGuard Home as DNS — see section 5a and 5b.

### VIRTUAL_PORT

nginx-proxy defaults to port 80 when generating the upstream. If your container
listens on a different port, set `VIRTUAL_PORT` explicitly:

```yaml
environment:
  - VIRTUAL_HOST=dozzle.local
  - VIRTUAL_PORT=8080   # dozzle listens on 8080, not 80
```

Without this, nginx-proxy will proxy to port 80 and the app will return a
connection refused or 502.

### VoidTower Proxy Manager integration

nginx-proxy's container conf.d directory is bind-mounted to
`/var/lib/voidtower/nginx/conf.d` on the host. VoidTower's Proxy Manager writes
files named `voidtower-{domain}.conf` into that directory alongside nginx-proxy's
auto-generated files (which nginx-proxy names after the virtual host).

The two sets of files coexist without conflict because they use different naming
schemes. nginx-proxy owns `<hostname>.conf`; VoidTower owns `voidtower-*.conf`.

### nginx vs Docker mode detection

VoidTower auto-detects whether nginx is running as a Docker container or a system
service by looking for a running container with the label
`com.docker.compose.project=vt-nginx-proxy`. If found it uses the Docker API to
reload nginx; otherwise it falls back to `systemctl reload nginx`.

---

## 4. Port Reference

| App              | Host Port(s)          | Container Port(s) | Notes                                          |
|------------------|-----------------------|-------------------|------------------------------------------------|
| adguardhome      | 5354/tcp+udp          | 53                | DNS; 3000 admin UI; 3001:80 alt                |
|                  | 3000                  | 3000              | Setup wizard (first run only)                  |
|                  | 3001                  | 80                | Admin UI via nginx-proxy                       |
| authentik        | 9443                  | 9443              | HTTPS direct access                            |
|                  | 9080                  | 9000              | HTTP direct access                             |
| changedetection  | 5000                  | 5000              | Web UI                                         |
| code-server      | 8444                  | 8443              | VS Code in browser (TLS)                       |
| comfyui          | 8188                  | 8188              | ComfyUI web interface                          |
| dozzle           | 8889                  | 8080              | Docker log viewer                              |
| freshrss         | 8082                  | 80                | RSS aggregator                                 |
| gitea            | 3002                  | 3000              | Git web UI                                     |
|                  | 2222                  | 22                | Git over SSH                                   |
| grafana          | 3005                  | 3000              | Dashboards                                     |
| immich           | 2283                  | 3001              | Photo management                               |
| jellyfin         | 8096                  | 8096              | Media server                                   |
| jitsi            | 8083                  | 80                | Video conference HTTP                          |
|                  | 8445                  | 443               | Video conference HTTPS                         |
|                  | 10000/udp             | 10000/udp         | WebRTC media                                   |
| kavita           | 5001                  | 5000              | E-book reader                                  |
| llama-cpp        | 8090                  | 8080              | llama.cpp OpenAI-compatible API                |
| matrix-synapse   | 8008                  | 8008              | Matrix federation HTTP                         |
|                  | 8448                  | 8448              | Matrix federation HTTPS                        |
| mealie           | 9925                  | 9000              | Recipe manager                                 |
| minio            | 9002                  | 9000              | S3 API                                         |
|                  | 9001                  | 9001              | MinIO console UI                               |
| n8n              | 5678                  | 5678              | Workflow automation                            |
| navidrome        | 4533                  | 4533              | Music streaming                                |
| nextcloud        | 8081                  | 80                | Cloud storage                                  |
| nginx-proxy      | 8080                  | 80                | HTTP reverse proxy (all apps)                  |
|                  | 8443                  | 443               | HTTPS reverse proxy                            |
| odysseus         | 7000                  | 7000              | VoidTower AI agent API                         |
| ollama           | 11434                 | 11434             | Ollama model API                               |
| open-webui       | 3003                  | 8080              | LLM chat interface                             |
| outline          | 3004                  | 3000              | Knowledge base                                 |
| paperless        | 8000                  | 8000              | Document management                            |
| pihole           | 5353/tcp+udp          | 53                | DNS; mapped away from 53 for systemd-resolved  |
|                  | 8180                  | 80                | Pi-hole admin UI (direct)                      |
| portainer        | 9000                  | 9000              | Docker management UI                           |
| redroid          | 5555                  | 5555              | ADB interface                                  |
|                  | 6080                  | 6080              | noVNC browser access                           |
| searxng          | 8891                  | 8080              | Meta search engine                             |
| stirling-pdf     | 8484                  | 8080              | PDF tools                                      |
| syncthing        | 8384                  | 8384              | Syncthing web UI                               |
|                  | 22000/tcp+udp         | 22000             | Syncthing sync protocol                        |
| uptime-kuma      | 3006                  | 3001              | Uptime monitoring                              |
| vaultwarden      | 8085                  | 80                | Bitwarden-compatible password manager          |
| vikunja          | 3456                  | 3456              | Task management                                |
| VoidTower        | 8743                  | 8743              | VoidTower management UI and API                |
| wireguard-easy   | 51820/udp             | 51820/udp         | WireGuard VPN                                  |
|                  | 51821/tcp             | 51821/tcp         | WireGuard Easy web UI                          |
| youkidex         | 8888                  | 6080              | Android emulator noVNC                         |

Pi-hole and AdGuard both avoid port 53 to prevent conflicts with
`systemd-resolved`. On a dedicated server without systemd-resolved, change
5353/5354 back to 53.

---

## 5. Alternative Setups

### 5a. Pi-hole as DNS

Instead of `/etc/hosts` entries on every client, set Pi-hole as your router's
upstream DNS server. Then add local DNS records in Pi-hole's admin UI
(Local DNS > DNS Records):

```
gitea.local     → <host-ip>
jellyfin.local  → <host-ip>
nextcloud.local → <host-ip>
# etc. for every VIRTUAL_HOST value
```

Pi-hole's own admin UI is reachable directly at `http://<host-ip>:8180` or
through nginx-proxy at `http://pihole.local:8080` (VIRTUAL_HOST is set in
`pihole.yml`).

Pi-hole's DNS listener is on host port 5353 (not 53) to avoid conflicting with
`systemd-resolved`. Point your router's DNS at `<host-ip>:5353`, or if your
router does not support non-standard DNS ports, either disable
systemd-resolved's stub listener or remap pihole to port 53 in `pihole.yml`.

### 5b. AdGuard Home instead of Pi-hole

AdGuard Home works the same way. Add local DNS rewrites in Settings > Filters >
DNS Rewrites:

```
*.local → <host-ip>   # wildcard to cover all apps at once
```

AdGuard ports:

| Host port | Purpose              |
|-----------|----------------------|
| 5354/tcp+udp | DNS (avoids systemd-resolved) |
| 3000      | Setup wizard (first run) |
| 3001      | Admin UI             |

The blocklist syntax differs from Pi-hole (AdGuard uses its own filter format
or standard hosts-file format). Upstream DNS config is in Settings > DNS
Settings > Upstream DNS servers.

### 5c. Traefik instead of nginx-proxy

Replace `nginx-proxy.yml` with a Traefik compose file. The key differences:

**Labels instead of env vars.** Remove `VIRTUAL_HOST` and `VIRTUAL_PORT` from
every app YAML and replace with Traefik labels:

```yaml
labels:
  - "traefik.enable=true"
  - "traefik.http.routers.myapp.rule=Host(`myapp.local`)"
  - "traefik.http.services.myapp.loadbalancer.server.port=8080"
```

**Port layout.** Traefik defaults to 80/443, not 8080/8443. Either remap Traefik
to 8080/8443 or update client DNS/hosts accordingly.

**No conf.d bind-mount equivalent.** The
`/var/lib/voidtower/nginx/conf.d` bind-mount has no direct counterpart in
Traefik. VoidTower's Proxy Manager writes nginx config fragments; with Traefik
you would need to use Traefik's file provider (a watched directory of TOML/YAML
router definitions) or the Traefik HTTP API to achieve the same dynamic rule
injection.

**Automatic HTTPS.** Traefik has built-in ACME support. For a homelab with no
public domain, use a self-signed cert or a local CA with `tls.certificates`.

### 5d. Caddy instead of nginx-proxy

Use `lucaslorentz/caddy-docker-proxy`. Labels on each service:

```yaml
labels:
  - "caddy=myapp.local"
  - "caddy.reverse_proxy={{upstreams 8080}}"
```

Caddy defaults to port 80/443. It generates self-signed certs automatically
for local names, or can use ACME with a DNS challenge for wildcard certs — no
manual cert management needed.

The same conf.d bind-mount caveat applies as with Traefik: there is no
equivalent mechanism for VoidTower's Proxy Manager to inject rules. File provider
or the Caddy admin API would be the workaround.

### 5e. No reverse proxy (direct port access)

Remove or skip `nginx-proxy.yml`. Access every app directly using the host
port from section 4:

```
http://<host-ip>:8096   # Jellyfin
http://<host-ip>:3002   # Gitea
http://<host-ip>:8000   # Paperless
```

The `VIRTUAL_HOST` and `VIRTUAL_PORT` env vars in each app YAML are harmless
if nginx-proxy is not running — they just go unused. You can leave them in place
or remove them.

Limitations: no hostname routing, no shared TLS termination, every app needs
its own port remembered. The port reference table in section 4 becomes your
primary navigation tool.

### 5f. Tailscale as VPN overlay

Install Tailscale on the VoidTower host:

```bash
curl -fsSL https://tailscale.com/install.sh | sh
tailscale up
```

Clients on the Tailscale network reach VoidTower apps via the host's Tailscale
IP (`100.x.x.x`):

```
http://100.x.x.x:8080   # nginx-proxy, then hostname routing works as normal
http://100.x.x.x:8096   # Jellyfin direct
```

With Tailscale MagicDNS enabled, the host gets a stable DNS name
(e.g. `voidtower.your-tailnet.ts.net`) and you can use `tailscale serve` to
expose individual apps:

```bash
tailscale serve --bg http://localhost:8080
```

The `tailscale.yml` app-vault entry runs Tailscale as a sidecar container with
the Docker socket mounted, providing Tailscale connectivity from within the
container network.

Nothing changes in the Docker networking — Tailscale is a transport layer above
it. All vt-proxy and nginx-proxy config stays the same.

### 5g. WireGuard (wireguard-easy)

`wireguard-easy.yml` runs a WireGuard server accessible at:

| Host port    | Purpose               |
|--------------|-----------------------|
| 51820/udp    | WireGuard protocol    |
| 51821/tcp    | wireguard-easy web UI |

Clients on the WireGuard subnet (`10.8.0.0/24` by default) reach the VoidTower
host via its WireGuard interface IP (typically `10.8.0.1`).

**DNS inside WireGuard.** Set the WireGuard peer's DNS to either:
- Pi-hole's container IP on vt-proxy (inspect with `docker network inspect vt-proxy`)
- The host IP on port 5353 if your WireGuard client supports non-standard DNS ports
- The host's primary interface IP if Pi-hole is remapped to port 53

In wireguard-easy this is the "DNS Server" field in each peer config or the
`WG_DEFAULT_DNS` env var in the compose file.

**AllowedIPs.** To route all traffic through VoidTower set `0.0.0.0/0` in peer
configs. For split-tunnel (only reach VoidTower apps) use the WireGuard subnet
plus the host's LAN subnet.

---

## 6. Change Checklist by Scenario

| Scenario | app-vault YAMLs | nginx-proxy.yml | install.sh | Client config |
|---|---|---|---|---|
| Pi-hole DNS | No change | No change | No change | Point router DNS to host:5353 |
| AdGuard Home DNS | No change | No change | No change | Point router DNS to host:5354 |
| Traefik | Remove `VIRTUAL_HOST`, `VIRTUAL_PORT`; add Traefik labels | Replace with Traefik compose | No change | Update port if changed from 8080 |
| Caddy | Remove `VIRTUAL_HOST`, `VIRTUAL_PORT`; add Caddy labels | Replace with caddy-docker-proxy | No change | Update port if changed from 8080 |
| No reverse proxy | Leave env vars in place (harmless) | Remove or stop | No change | Use direct host ports from section 4 |
| Tailscale | No change | No change | No change | Install Tailscale client; use Tailscale IP |
| WireGuard | No change | No change | No change | Configure peer with WireGuard subnet DNS |

---

## 7. Troubleshooting

**Container can't reach another container**

Check both containers are attached to vt-proxy:

```bash
docker network inspect vt-proxy --format '{{range .Containers}}{{.Name}} {{end}}'
```

If a container is missing, its YAML may be missing the top-level `networks:`
declaration (the bug this document was written alongside). Re-deploy the app
through VoidTower or run `docker compose up -d` from the app directory.

**nginx-proxy returns 503 or doesn't route**

1. Confirm `VIRTUAL_HOST` is set on the target container:
   ```bash
   docker inspect <container> | grep VIRTUAL_HOST
   ```
2. Confirm the container is on vt-proxy (see above).
3. Check nginx-proxy logs: `docker logs vt-nginx-proxy`.
4. If `VIRTUAL_PORT` is unset and the app does not listen on 80, add it.

**Port already in use on startup**

Cross-reference section 4. Common conflicts:
- Port 53: disable `systemd-resolved`'s stub listener (`/etc/systemd/resolved.conf`: `DNSStubListener=no`) or use the 5353/5354 mappings already in the YAMLs.
- Port 8080: nginx-proxy owns this. llama-server was previously mapped to 8080 and is now on 8090.
- Port 80/443: if a system nginx is running, either stop it or don't bind nginx-proxy to 80/443 (use 8080/8443 as VoidTower does by default).

**DNS not resolving `appname.local`**

1. Verify Pi-hole or AdGuard is running: `docker ps | grep pihole` or `docker ps | grep adguard`.
2. Verify your system is using it as DNS: `resolvectl status` or `cat /etc/resolv.conf`.
3. Verify the DNS record exists in Pi-hole (Local DNS > DNS Records) or AdGuard (Filters > DNS Rewrites).
4. Fallback: add the entry manually to `/etc/hosts` on the client.

**llama-server not reachable at expected URL**

The llama-server systemd service runs on port 8090 (changed from 8080 to avoid
conflict with nginx-proxy). The config written by `install.sh` uses
`http://127.0.0.1:8090/v1`. If you installed before this change, update the
service file:

```bash
sudo sed -i 's/--port 8080/--port 8090/' /etc/systemd/system/voidtower-llama.service
sudo systemctl daemon-reload
sudo systemctl restart voidtower-llama.service
```

And update `/var/lib/voidtower/config.json` to replace `8080` with `8090` in the
`llm_base_url` field.
