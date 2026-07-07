# Installation Guide

## Quick start

### Docker (recommended)

```bash
git clone -b main https://github.com/niwlekakan/voidtower
cd voidtower
cp .env.example .env          # set ODYSSEUS_ADMIN_PASSWORD at minimum
docker compose --profile aio up -d
```

### Bare metal / VM — VoidTower only

```bash
curl -fsSL https://raw.githubusercontent.com/niwlekakan/voidtower/main/scripts/install.sh \
  | sudo bash
```

### Bare metal / VM — Full AIO stack

```bash
curl -fsSL https://raw.githubusercontent.com/niwlekakan/voidtower/main/scripts/install.sh \
  | sudo bash -s -- --all-in-one --pull-model

# Non-interactive with specific model
sudo bash install.sh \
  --all-in-one \
  --ai-model qwen2.5-coder:7b-instruct \
  --pull-model \
  --yes
```

---

## Installer flags

### Stack composition

| Flag | Description |
|---|---|
| `--all-in-one` | Shorthand for `--with-odysseus --with-voidwatch --with-ai` |
| `--with-odysseus` | Install Odysseus AI workspace |
| `--with-voidwatch` | Wire Voidwatch integration (implies `--with-odysseus`) |
| `--with-ai` | Set up Ollama local AI runtime |
| `--ai-model MODEL` | Model to configure (e.g. `qwen2.5-coder:7b-instruct`) |
| `--pull-model` | Pull the model during install |

### Ports & behaviour

| Flag | Description |
|---|---|
| `--odysseus-port PORT` | Odysseus port (default: 7000) |
| `--port PORT` | VoidTower port (default: 8743) |
| `--yes` | Non-interactive |
| `--dry-run` | Preview what would happen |
| `--offline` | Skip network calls |
| `--musl` | Build a fully-static musl binary — use on TrueNAS Scale, Alpine, or any platform with an old glibc |

### Skip flags

| Flag | Description |
|---|---|
| `--no-mcp` | Skip MCP tool registration |
| `--no-webhooks` | Skip webhook configuration |
| `--no-toolpacks` | Skip toolpack installation |

### Maintenance flags

| Flag | Description |
|---|---|
| `--uninstall` | Remove VoidTower — interactively choose what to keep (data, config, system user) |
| `--reset` | Wipe state (database, config, secrets, bootstrap token) and restart — binary and service unit kept |
| `--repair` | Re-download binary, reinstall service unit, fix ownership/permissions, restart |
| `--update` | Pull latest (or `--version`) binary, refresh app catalog, restart — data and config untouched |

---

## Model auto-selection

The installer recommends a model based on available RAM when `--pull-model` is set:

| RAM | Recommended model |
|---|---|
| ≥ 32 GB | `qwen2.5-coder:14b-instruct` |
| ≥ 16 GB | `qwen2.5-coder:7b-instruct` |
| ≥ 8 GB | `qwen2.5-coder:3b-instruct` |
| < 8 GB | No auto-pull — configure manually or use a remote endpoint |

---

## After installation

1. Open VoidTower at `http://localhost:8743/bootstrap` (bare metal) or `https://localhost` (Docker)
2. Complete the setup wizard — creates your admin account and consumes the bootstrap token
3. Open Odysseus at `http://localhost:7000` and log in
4. Go to **Settings → Integrations → Voidwatch** to confirm the connection shows green

Bootstrap credentials are saved to:
- `/root/voidtower-bootstrap-token`
- `/root/odysseus-bootstrap-token`
- `/root/voidwatch-recovery-info`

---

## Service management (bare metal)

```bash
systemctl status voidtower odysseus ollama

journalctl -u voidtower -f
journalctl -u odysseus -f
journalctl -u ollama -f

systemctl restart voidtower
systemctl restart odysseus
```

---

## Platform-specific guides

- [TrueNAS Scale](../platforms/truenas.md)
- [Proxmox LXC](../platforms/proxmox-lxc.md)
- [GPU & Ollama](../gpu.md)
