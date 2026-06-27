# App Vault Catalog

The App Vault catalog is a directory of YAML files — one per deployable app — that VoidTower reads at startup. The catalog directory is set via the `VOIDTOWER_CATALOG_DIR` environment variable (default: `app-vault/apps/`).

---

## Adding a custom app

Drop a `.yml` file into the catalog directory. VoidTower picks it up on next restart (or on catalog refresh from **App Vault → Refresh**).

Minimal example:

```yaml
id: my-app
name: My App
description: A self-hosted app
category: tools
compose:
  services:
    my-app:
      image: myorg/my-app:latest
      restart: unless-stopped
      ports:
        - "9000:9000"
      volumes:
        - my_app_data:/data
  volumes:
    my_app_data:
```

---

## Full schema

```yaml
# Required
id: gitea                         # Unique identifier — used as project name prefix on deploy
name: Gitea                       # Display name shown in the catalog
description: Self-hosted Git service
category: dev                     # Freeform — used for filtering: dev, media, monitoring, tools, ai, ...

# Optional metadata
icon: https://gitea.io/images/gitea.png   # URL shown as the app icon in the catalog
version_hint: "latest"            # Informational — shown in the UI, not enforced

# Docker Compose definition (required)
compose:
  services:
    gitea:
      image: gitea/gitea:latest
      restart: unless-stopped
      ports:
        - "3002:3000"
        - "2222:22"
      volumes:
        - gitea_data:/data
      environment:
        - USER_UID=1000
        - USER_GID=1000
        - GITEA__security__SECRET_KEY=${GITEA_SECRET_KEY}   # Reference required_env vars with ${VAR}
        - VIRTUAL_HOST=gitea.local    # nginx-proxy routing hint (optional)
        - VIRTUAL_PORT=3000
      networks:
        default: {}
        vt-proxy:                 # Join the shared nginx-proxy network (optional)
          aliases:
            - gitea
  volumes:
    gitea_data:
  networks:
    vt-proxy:
      external: true             # Must be external: true for vt-proxy

# Environment variables (optional)
# Shown to the user in the pre-deploy modal; values can be overridden before deploy
required_env:
  - key: GITEA_SECRET_KEY
    description: "Session and CSRF secret key"
    generate: random_hex_64      # Auto-generated if not overridden — see generation types below
  - key: GITEA_ADMIN_PASSWORD
    description: "Initial admin password"
    # No generate: field — user must supply this value

# AI integration metadata (optional)
# Informs Odysseus of what it can do with this app
ai_integration:
  level: native                  # native | aware | none
  description: "Odysseus can manage repositories, issues, branches, and webhooks"

# Links shown on the app's detail page (optional)
links:
  home: https://gitea.io
  docs: https://docs.gitea.com
```

---

## Environment variable generation types

| Value | What gets generated |
|---|---|
| `random_hex_64` | 64-character random hex string |
| `random_hex_32` | 32-character random hex string |
| `random_base64_32` | 32-byte random value, base64-encoded |
| `uuid` | Random UUID v4 |

If no `generate` is specified, the user must provide the value in the pre-deploy modal.

---

## nginx-proxy integration

Apps can join the shared `vt-proxy` Docker network for automatic nginx routing:

```yaml
services:
  my-app:
    environment:
      - VIRTUAL_HOST=my-app.local
      - VIRTUAL_PORT=8080
    networks:
      default: {}
      vt-proxy:
        aliases:
          - my-app

networks:
  vt-proxy:
    external: true
```

The nginx-proxy container must be deployed first from **App Vault → nginx-proxy**. Without it, adding the network is harmless — the app just won't have automatic routing.

---

## AI integration levels

The `ai_integration.level` field tells Odysseus what it should expect when interacting with this app:

| Level | Meaning |
|---|---|
| `native` | Odysseus has direct API-level integration (e.g. via a Voidwatch toolpack) |
| `aware` | Odysseus can read stats or status but has limited control |
| `none` | No integration — Odysseus can only see the container via Docker |

This is metadata only — it does not affect deploy behaviour.

---

## Deploy target

By default, apps deploy to the local Docker daemon. Apps can also be deployed to a Proxmox LXC container — see [docs/integrations/proxmox.md](integrations/proxmox.md#deploy-to-proxmox-lxc-from-app-vault).

---

## Categories in the built-in catalog

| Category | Examples |
|---|---|
| `media` | Jellyfin, Jellyseerr, Bazarr, Lidarr, Kavita |
| `dev` | Gitea, Code Server |
| `monitoring` | Grafana, Uptime Kuma, Dozzle |
| `ai` | Ollama, Open WebUI, ComfyUI |
| `networking` | Pi-hole, AdGuard Home, Gluetun, Flaresolverr |
| `storage` | MinIO, Nextcloud, Immich, Paperless |
| `automation` | n8n, Home Assistant, Changedetection |
| `tools` | Vaultwarden, Portainer, Authentik, Jitsi |
