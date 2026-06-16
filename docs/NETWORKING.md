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
| immich           | 2283                  | 2283              | Photo management                               |
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

#### First-run setup

Unlike AdGuard, Pi-hole v6 has no interactive setup wizard — everything is
configured via env vars in `pihole.yml` before first deploy:

- **Admin UI password:** `FTLCONF_webserver_api_password=changeme` — change
  this before deploying (or update it later in Settings > Web Interface, or
  via `docker exec pihole pihole setpassword`).
- **Upstream DNS:** `FTLCONF_dns_upstreams=1.1.1.1;8.8.8.8` — semicolon-
  separated list of resolvers Pi-hole forwards to.

There's no "listen interface" step and no `127.x.x.x` candidate-address list
like AdGuard's wizard — Pi-hole binds inside the container to all interfaces
on the ports declared in `pihole.yml` (`5353:53` for DNS, `8180:80` for the
admin UI), so no extra configuration is needed there.

#### Pointing your router's DNS at Pi-hole

Use the **TrueNAS/host's LAN IP** (e.g. `192.168.1.x`), not a container-
internal address — that's the IP the `5353:53` mapping is published on.

Pi-hole's DNS listener is on host port 5353 (not 53) to avoid conflicting with
`systemd-resolved`. Most routers accept a DNS server *IP* but not a custom
*port*, so `<host-ip>:5353` often isn't usable directly. Two options:

1. **Remap Pi-hole to host port 53** in `pihole.yml` (change `"5353:53/tcp"` /
   `"5353:53/udp"` to `"53:53/..."`), after disabling `systemd-resolved`'s
   stub listener (`/etc/systemd/resolved.conf`: `DNSStubListener=no`, then
   `systemctl restart systemd-resolved`) — see section 7. **Does not apply to
   TrueNAS** — see below.
2. Set DNS to `<host-ip>:5353` on a per-device basis, if the OS/client
   supports a custom DNS port (most routers don't, but some OSes/apps do).

On a typical Linux box, option 1 is the common choice since the router is
usually the one place a custom DNS port isn't supported.

#### TrueNAS: port 53 is owned by dnsmasq, not systemd-resolved

TrueNAS SCALE has no `/etc/systemd/resolved.conf` — port 53 on the host is
held by TrueNAS's own `dnsmasq` (used for VM/bridge networking and the apps/
k3s network). **Don't disable or remap it** — that can break VM networking and
App Vault's container networking. So option 1 above doesn't apply on TrueNAS.

If your router can't use a custom DNS port, two TrueNAS-friendly alternatives:

**A. Give Pi-hole/AdGuard its own IP via macvlan (recommended)**

```bash
docker network create -d macvlan \
  --subnet=192.168.1.0/24 \
  --gateway=192.168.1.1 \
  -o parent=eth0 \
  pihole-macvlan
```

Attach the container to `pihole-macvlan` with a static IP (e.g.
`192.168.1.53`) instead of publishing host ports. It binds port 53 on **its
own IP**, with zero conflict with TrueNAS's dnsmasq (bound to the TrueNAS
host's IP). Point your router's DNS at `192.168.1.53` — standard port 53,
works with any router.

Caveat: the TrueNAS host itself usually can't reach a macvlan container
directly (a known Docker macvlan limitation) — not a problem here since DNS
clients are other LAN devices, not the host.

**B. DNAT port-redirect on the host**

Add an iptables rule via TrueNAS's persistent Init/Shutdown Scripts (System
Settings → Advanced → Init/Shutdown Scripts, type: Command, run at Post Init)
to redirect inbound port 53 to Pi-hole's `5353`:

```bash
iptables -t nat -A PREROUTING -i <lan-iface> -p udp --dport 53 -j REDIRECT --to-port 5353
iptables -t nat -A PREROUTING -i <lan-iface> -p tcp --dport 53 -j REDIRECT --to-port 5353
```

This intercepts packets before dnsmasq's socket sees them, so no conflict —
but it's more fragile (only persists via the init script, and `<lan-iface>`
must be the correct LAN-facing interface name).

Option A is generally preferred — no iptables rules to maintain across
reboots/upgrades, and router config stays at a normal port-53 IP.

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

#### First-run setup wizard

The wizard asks for a **Listen interface**, **Web interface port**, and **DNS
server port**. These are all *container-internal* — pick values consistent
with the port mappings in `adguardhome.yml`:

- **Listen interface:** "All interfaces" / `0.0.0.0`
- **DNS server port:** `53` — maps to host port `5354` via the existing
  `5354:53` mapping
- **Web interface port:** keep `3000` (matches `VIRTUAL_PORT=3000` in the
  YAML — admin UI stays reachable at `<host-ip>:3000` and at
  `adguard.local:8080` via nginx-proxy with no extra config). Choosing `80`
  instead moves the admin UI to `<host-ip>:3001` (via the `3001:80` mapping)
  but leaves `VIRTUAL_PORT=3000` stale — `adguard.local:8080` would 502 until
  you redeploy with `VIRTUAL_PORT=80` (same bug class as section 8).

The wizard also lists some `127.x.x.x` / container-internal IPs as candidate
listen addresses — these are from inside the AdGuard container's network
namespace and are **not reachable from your router or LAN devices**. Ignore
them.

#### Pointing your router's DNS at AdGuard

Use the **TrueNAS/host's LAN IP** (e.g. `192.168.1.x`), not anything from the
wizard's interface list — that's the IP the `5354:53` mapping is published
on.

Most routers accept a DNS server *IP* but not a custom *port*, so
`<host-ip>:5354` often isn't usable directly. Two options:

1. **Remap AdGuard to host port 53** in `adguardhome.yml` (change
   `"5354:53/tcp"` / `"5354:53/udp"` to `"53:53/..."`), after disabling
   `systemd-resolved`'s stub listener (`/etc/systemd/resolved.conf`:
   `DNSStubListener=no`, then `systemctl restart systemd-resolved`) — see
   section 7. **Does not apply to TrueNAS** — see the "TrueNAS: port 53 is
   owned by dnsmasq" callout in section 5a, which applies equally here
   (substitute AdGuard's `5354` for Pi-hole's `5353`).
2. Set DNS to `<host-ip>:5354` on a per-device basis, if the OS/client
   supports a custom DNS port (most routers don't, but some OSes/apps do).

On a typical Linux box, option 1 is the common choice since the router is
usually the one place a custom DNS port isn't supported.

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

---

## 8. Catalog YAML Checklist — Wiring Up an App

For any app-vault YAML with a web UI, the service that actually serves HTTP
needs **all three** of the following, on the **same service block**:

```yaml
services:
  myapp:                          # <-- the service with the published web port
    ports:
      - "1234:80"                 # host:container — VIRTUAL_PORT must match the container side
    environment:
      - VIRTUAL_HOST=myapp.local
      - VIRTUAL_PORT=80           # omit only if the container listens on port 80
    networks:
      default: {}
      vt-proxy:
        aliases:
          - myapp
```

A common bug pattern (found and fixed in `immich.yml`, `authentik.yml`,
`jitsi.yml`, `outline.yml`, `paperless.yml`) is **copy-pasting the `vt-proxy` +
`aliases` block onto a sidecar container** (redis, postgres, a worker) instead
of the web-facing service. The symptom: the app is reachable at
`http://<host-ip>:<port>` (Docker's port publishing doesn't care about
vt-proxy) but **not** at `http://myapp.local:8080` — nginx-proxy and the app
container aren't on a shared network, so nginx-proxy can't reach it even
though it sees the `VIRTUAL_HOST` label via the Docker socket.

To audit any app-vault YAML for this:

1. Find the service with `VIRTUAL_HOST` in its `environment`.
2. Confirm that *same service* has `networks.vt-proxy` declared.
3. Confirm `VIRTUAL_PORT` matches the container-side port in `ports:` (the
   number after the colon), not the host-side port.

Apps with **no web UI** (CLI tools, sidecars like `recyclarr`, VPN containers
like `gluetun`) should set `no_web_ui: true` instead of `VIRTUAL_HOST` —
VoidTower's UI then skips showing an "Open" button. Apps using
`network_mode: host` (e.g. `homeassistant`) intentionally skip nginx-proxy
entirely — host-networked containers don't share a Docker network with
nginx-proxy, so access them directly via `http://<host-ip>:<port>`.

---

## 9. Generic Access Recipe — Reaching Any App

Once an app's YAML passes the checklist in section 8, here's how to reach it
through each layer. Substitute `myapp` / `<port>` for the real
`VIRTUAL_HOST` value and host port from section 4.

| Access method | URL / config | Requirements |
|---|---|---|
| **Direct port (LAN)** | `http://<host-ip>:<port>` | Always works once the container is up — no extra config |
| **nginx-proxy (LAN, hostname)** | `http://myapp.local:8080` | Section 8 checklist satisfied + `myapp.local` resolves to `<host-ip>` on the client |
| **Pi-hole DNS** | same as above | Local DNS > DNS Records: `myapp.local → <host-ip>` (section 5a) |
| **AdGuard DNS** | same as above | Filters > DNS Rewrites: `myapp.local → <host-ip>` or wildcard `*.local → <host-ip>` (section 5b) |
| **VoidTower Proxy Manager (custom domain / TLS)** | `https://myapp.example.com` | Add a Proxy entry with upstream `http://localhost:<port>` — VoidTower rewrites `localhost` to the Docker host IP automatically |
| **Tailscale (remote, direct port)** | `http://<tailscale-ip>:<port>` | Tailscale running on the host — no DNS needed, bypasses nginx-proxy entirely |
| **Tailscale (remote, hostname routing)** | `http://myapp.local:8080` over the tailnet | Set Pi-hole/AdGuard as the tailnet's DNS (Tailscale admin > DNS > Nameservers), or `tailscale serve --bg http://localhost:8080` |
| **WireGuard (remote, direct port)** | `http://10.8.0.1:<port>` | Connected to wireguard-easy's VPN subnet — no DNS needed |
| **WireGuard (remote, hostname routing)** | `http://myapp.local:8080` over the VPN | Set the WireGuard peer's DNS to Pi-hole/AdGuard's host IP:5353/5354 (or vt-proxy IP) |

**Rule of thumb:** direct-port and Tailscale/WireGuard direct-port access work
for *any* app regardless of section 8 — they bypass nginx-proxy and just hit
the published Docker port. Hostname-based access (`myapp.local`) always
requires both the section 8 checklist *and* a DNS record/rewrite somewhere in
the chain (Pi-hole, AdGuard, `/etc/hosts`, or Tailscale MagicDNS).
