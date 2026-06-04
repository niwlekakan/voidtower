# All-in-One Install — VoidTower + Odysseus + Voidwatch + AI

## Quickstart

```bash
curl -fsSL https://raw.githubusercontent.com/elwla/voidtower/main/scripts/install.sh \
  | sudo bash -s -- --all-in-one --pull-model
```

This installs:
- **VoidTower** — infrastructure control plane (port 8743)
- **Odysseus** — AI workspace (port 7000)
- **Voidwatch** — AI ops integration between the two
- **Ollama** — local AI runtime (port 11434)
- A recommended model based on your RAM

---

## Common Commands

### VoidTower only
```bash
sudo bash install.sh
```

### VoidTower + Odysseus
```bash
sudo bash install.sh --with-odysseus
```

### Full stack with local AI
```bash
sudo bash install.sh --all-in-one --ai-provider ollama --pull-model
```

### Full stack, specific model, non-interactive
```bash
sudo bash install.sh \
  --all-in-one \
  --ai-model qwen2.5-coder:7b-instruct \
  --pull-model \
  --yes
```

### Dry run (preview without changes)
```bash
sudo bash install.sh --all-in-one --dry-run
```

### Offline (no downloads, local package manager only)
```bash
sudo bash install.sh --offline
```

---

## Installer Flags

| Flag | Description |
|------|-------------|
| `--all-in-one` | Shorthand for `--with-odysseus --with-voidwatch --with-ai` |
| `--with-odysseus` | Install Odysseus AI workspace |
| `--with-voidwatch` | Install Voidwatch integration (implies `--with-odysseus`) |
| `--with-ai` | Set up local AI runtime |
| `--ai-provider` | `ollama` (default) \| `openai-compatible` \| `none` |
| `--ai-model` | Ollama model name (e.g. `qwen2.5-coder:7b-instruct`) |
| `--pull-model` | Pull the model during install |
| `--skip-model-pull` | Never pull a model |
| `--odysseus-port` | Odysseus port (default: 7000) |
| `--port` | VoidTower port (default: 8743) |
| `--yes` | Non-interactive, assume yes |
| `--dry-run` | Print what would happen, make no changes |
| `--offline` | No network except local package manager |
| `--no-mcp` | Skip MCP tool registration |
| `--no-webhooks` | Skip webhook configuration |
| `--no-toolpacks` | Skip toolpack installation |

---

## Model Selection

The installer automatically recommends a model based on available RAM:

| RAM | Recommended Model |
|-----|------------------|
| ≥ 32 GB | `qwen2.5-coder:14b-instruct` |
| ≥ 16 GB | `qwen2.5-coder:7b-instruct` |
| ≥ 8 GB | `qwen2.5-coder:3b-instruct` |
| < 8 GB | No auto-pull — configure manually |

Override with `--ai-model <name>`. Large models are never pulled without `--pull-model` or interactive confirmation.

---

## After Installation

1. Open VoidTower: `http://localhost:8743/bootstrap`
2. Complete the bootstrap setup (create admin account)
3. Open Odysseus: `http://localhost:7000`
4. Go to **Settings → Integrations → Voidwatch** to confirm the connection

Credentials are saved to:
- `/root/voidlink-bootstrap-token`
- `/root/odysseus-bootstrap-token`
- `/root/voidwatch-recovery-info`

---

## Service Management

```bash
# Status
systemctl status voidtower odysseus ollama

# Logs
journalctl -u voidtower -f
journalctl -u odysseus -f
journalctl -u ollama -f

# Restart
systemctl restart voidtower
systemctl restart odysseus
```

---

## Readiness Check

```bash
bash scripts/doctor.sh --with-odysseus
```

---

## Uninstall

```bash
# Remove VoidTower only (preserve data)
sudo bash scripts/uninstall.sh

# Remove VoidTower + Odysseus
sudo bash scripts/uninstall.sh --remove-odysseus

# Remove everything including data
sudo bash scripts/uninstall.sh --all --purge
```

---

## Emergency Disable (Voidwatch)

```bash
# Immediately stop all Voidwatch automation
curl -X POST http://localhost:7000/api/voidwatch/emergency-disable

# From VoidTower UI: Settings → Integrations → Odysseus → Emergency Disable
```

---

## Example Agent Prompts (after install)

Once Odysseus is running and Voidwatch is connected, try:

```
Check my servers and tell me what is unhealthy.
Restart failed non-critical containers only.
Inspect nginx routes and tell me what is publicly exposed.
Run backups on all configured backup jobs.
Investigate why Jellyfin is unhealthy.
Summarize all active alerts.
Dry-run an image update for FreshRSS and ask before applying.
```
