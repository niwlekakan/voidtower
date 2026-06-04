# VoidTower

**Self-hosted Linux infrastructure management — one dashboard to rule your homelab.**

VoidTower is an open-source, local-first server control plane. It gives you a polished web UI for everything you'd normally SSH in to do: containers, services, files, backups, monitoring, reverse proxies, and AI integration. One install script, no cloud dependency, no telemetry.

---

## Features

| Area | What you get |
|---|---|
| **Dashboard** | Customizable widgets — clock, weather, CPU/RAM/disk charts, container summary, alert count. Nine toggleable widgets, config persisted per-browser. |
| **Services** | Manage systemd units — start, stop, restart, view logs in real time. |
| **Containers** | Docker container list, start/stop/restart, log viewer, per-container exec shell, compose file editor with staged diff before apply. |
| **App Vault** | 22+ one-click app deployments (Gitea, Nextcloud, Jellyfin, Grafana, Pi-hole, n8n, Home Assistant, Authentik, and more). |
| **AI Discover** | Ask the configured LLM to recommend self-hosted apps; results include Docker image names and direct deploy buttons for catalog matches. |
| **Files** | Full filesystem browser — roots sidebar, breadcrumb navigation, inline rename/delete, text editor with Ctrl+S, 3 s live poll. |
| **Reverse Proxies** | nginx-backed proxy rule manager — domain + upstream + SSL, configs written to `/etc/nginx/sites-enabled/`, validated and reloaded automatically. |
| **Backups** | Restic-powered backup jobs — init repo, run now, list snapshots. |
| **Monitoring** | Status checks (TCP/HTTP), public `/status` page (no auth, auto-refreshes), alerts with ack/resolve flow. |
| **Security** | Session list for all users, revoke individual sessions or all-others, audit log for every action. |
| **Terminal** | Full PTY browser terminal. |
| **AI workspace** | Iframe-embed any OpenAI-compatible AI workspace (Odysseus, Open WebUI, etc.) — configure endpoint in Settings. |
| **Themes** | 7 built-in themes + custom token editor with live color pickers, import/export JSON. |
| **Animated backgrounds** | 8 presets (Void, Grid, Aurora, Pulse, Noise, Hex, Circuit, None) + 4 glass levels (Solid, Blur, Acrylic, Frosted). |
| **Mobile** | Responsive layout — hamburger sidebar drawer on small screens, touch-friendly tap targets. |

---

## Quick install

```bash
curl -fsSL https://raw.githubusercontent.com/elwla/voidtower/main/scripts/install.sh | bash
```

The installer will:

1. Detect your distro and install dependencies (Rust, Node.js, Docker).
2. Build the backend and frontend.
3. Install a systemd service (`voidtower.service`).
4. Prompt for network/discovery setup: localhost, mDNS (`hostname.local`), custom hostname, or public domain + nginx.
5. Optionally detect your GPU, download a matching GGUF model, set up llama.cpp as a service, and offer to install Odysseus AI workspace.

**Variables you can override:**

```bash
VT_PORT=8743 VT_DATA_DIR=/var/lib/voidtower bash install.sh
```

---

## Manual setup

### Prerequisites

- Rust 1.75+ (`rustup`)
- Node.js 20+
- Docker (optional — needed for containers/app vault)
- nginx (optional — needed for reverse proxy manager)
- restic (optional — needed for backups)

### Build

```bash
# Backend
cd backend
cargo build --release

# Frontend
cd ../frontend
npm install
npm run build
```

### Run (development)

```bash
# Backend (reads dev-config/config.toml)
cd backend && cargo run

# Frontend dev server
cd frontend && npm run dev
```

The backend serves the compiled frontend from `../frontend/dist` in production mode.

### Configuration

`/etc/voidtower/config.toml` (production) or `dev-config/config.toml` (dev):

```toml
data_dir   = "/var/lib/voidtower"
port       = 8743
host       = "0.0.0.0"
catalog_dir = "/etc/voidtower/app-vault"
apps_dir   = "/var/lib/voidtower/apps"
```

---

## First login

1. Open `http://localhost:8743` (or your configured domain/mDNS address).
2. You'll be redirected to `/bootstrap`.
3. Enter the **bootstrap token** printed by the installer (also stored in `/var/lib/voidtower/bootstrap_token`).
4. Choose a username (use the **Suggest** button for a themed codename) and a strong password.
5. The owner account is created; the bootstrap token is consumed.

> **Security note:** The default bootstrap password is shown during installation and must be changed on first login. Any admin-created account is flagged **force password change** and cannot use the app until credentials are updated.

---

## Pages

| Route | Description |
|---|---|
| `/dashboard` | Customizable widget grid |
| `/services` | systemd service management |
| `/containers` | Docker containers; click a row → detail page |
| `/containers/:id` | Compose editor, exec shell, logs |
| `/apps` | App Vault catalog + Deployed list + AI Discover |
| `/files` | Filesystem browser and editor |
| `/proxies` | nginx reverse proxy rules |
| `/backups` | Restic backup jobs |
| `/alerts` | Active alerts + status checks |
| `/security` | Sessions + audit |
| `/ai` | Embedded AI workspace |
| `/terminal` | Browser PTY |
| `/audit` | Audit log |
| `/settings` | Theme, users, AI endpoints |
| `/status` | **Public** status page (no login required) |

---

## API

All endpoints are under `/api/*` and require a valid `vt_session` cookie except `/api/health`, `/api/auth/login`, `/api/auth/bootstrap`, and `/status`.

### Auth

```
POST /api/auth/bootstrap   { token, username, password }
POST /api/auth/login       { username, password }
POST /api/auth/logout
GET  /api/auth/me
```

### Metrics

```
GET  /api/metrics/current          JSON snapshot
GET  /api/metrics/ws               WebSocket stream (1 s interval)
```

### Services

```
GET  /api/services
GET  /api/services/:name
POST /api/services/:name/action    { action: start|stop|restart|enable|disable }
GET  /api/services/:name/logs
```

### Containers

```
GET  /api/containers
GET  /api/containers/images
POST /api/containers/:id/action    { action: start|stop|restart|remove }
GET  /api/containers/:id/logs
GET  /api/containers/:id/exec      WebSocket PTY (docker exec)
GET  /api/containers/:id/compose
POST /api/containers/:id/compose/propose   { content }  → diff stats
POST /api/containers/:id/compose/apply     { content }  → write + restart
```

### App Vault

```
GET  /api/apps/catalog
GET  /api/apps/deployed
POST /api/apps/deploy              { app_id, project_name?, env_overrides? }
POST /api/apps/:name/stop
GET  /api/apps/:name/compose
POST /api/apps/:name/compose       { content }  → write + restart
```

### Files

```
GET  /api/files/roots
GET  /api/files/list?path=
GET  /api/files/read?path=
POST /api/files/write              { path, content }
POST /api/files/mkdir              { path }
DELETE /api/files/delete?path=
POST /api/files/rename             { from, to }
GET  /api/files/activity
```

### Proxy

```
GET  /api/proxy
POST /api/proxy                    { domain, upstream, ssl }
DELETE /api/proxy/:id
POST /api/proxy/:id/toggle
```

### Backups

```
GET  /api/backups
POST /api/backups                  { name, source_path, repo_path, password }
POST /api/backups/:id/run
DELETE /api/backups/:id
```

### Status checks

```
GET  /api/status-checks
POST /api/status-checks            { name, type, target, interval_secs? }
DELETE /api/status-checks/:id
GET  /status                       Public HTML status page
```

### Users

```
GET  /api/users
POST /api/users                    { username, password, role }
DELETE /api/users/:id
POST /api/users/me/password        { password, username? }
```

### Security

```
GET  /api/security/sessions
POST /api/security/sessions/revoke-others
DELETE /api/security/sessions/:id
```

### Audit

```
GET  /api/audit?limit=&offset=
```

---

## Roles

| Role | Capabilities |
|---|---|
| `owner` | Everything, including deleting other admins |
| `admin` | Create/delete users (not owner), all actions |
| `operator` | Start/stop services, deploy apps, write files, terminal |
| `viewer` | Read-only access to all pages |

---

## Upgrading

```bash
cd /opt/voidtower   # or wherever you cloned it
git pull
cd backend && cargo build --release
cd ../frontend && npm ci && npm run build
sudo systemctl restart voidtower
```

---

## Uninstalling

```bash
sudo systemctl disable --now voidtower
sudo rm /etc/systemd/system/voidtower.service
sudo rm -rf /var/lib/voidtower   # removes all data including DB
sudo rm -rf /opt/voidtower       # removes the install
sudo systemctl daemon-reload
```

---

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md). The project follows AGPL-3.0 — contributions welcome.

Bug reports and feature requests: [GitHub Issues](https://github.com/elwla/voidtower/issues).

---

## License

AGPL-3.0 — see [LICENSE](LICENSE) or [NOTICE](NOTICE).
