# VoidTower on TrueNAS Scale

Two deployment paths depending on how much access you want. Both store all data on your TrueNAS datasets so nothing is lost across app updates.

> **Docker control disclaimer:** The TrueNAS Custom App UI (Option A) runs containers through its own Kubernetes layer (k3s) and does **not** expose `/var/run/docker.sock` to containers. VoidTower's container management panel, in-UI exec shell, and self-update feature require direct Docker socket access and will be unavailable. Everything else works normally: the dashboard, services, backups, proxies, secrets, Voidwatch AI integration, and all other pages. If you need container management, use Option B.

---

## Option A — Custom App UI

No SSH required. Uses TrueNAS's built-in app deployment.

**1. Create a dataset**

Go to **Storage → Add Dataset**, create a dataset named `voidtower` on your pool (e.g. `tank/voidtower`).

**2. Open Custom App**

Go to **Apps → Discover Apps → Custom App**.

**3. Paste the YAML**

Copy the contents of [`deploy/truenas/custom-app.yml`](../../deploy/truenas/custom-app.yml) into the YAML editor.

**4. Set environment variables**

| Variable | Value |
|---|---|
| `ODYSSEUS_ADMIN_PASSWORD` | your chosen password |
| `TRUENAS_POOL` | your ZFS pool name (e.g. `tank`) — run `zpool list` to find yours |
| `VOIDWATCH_TOKEN` | leave blank — fill in after first login |
| `VOIDWATCH_WEBHOOK_SECRET` | generate: `openssl rand -hex 32` |
| `SEARXNG_SECRET` | generate: `openssl rand -hex 32` |

**5. Deploy**

Click **Install**. TrueNAS will pull the images and start all services.

**6. First login**

Open `https://<truenas-ip>:8443` and accept the self-signed certificate. You'll be redirected to the bootstrap page — find your one-time token in the app logs:

```
Apps → voidtower → Logs → (select voidtower container)
```

Complete the setup wizard to create your admin account.

**7. Wire Voidwatch**

1. Go to **Settings → Integrations → API Tokens → New Token**
2. Create a token with [Voidwatch scopes](../../README.md#voidwatch-token-scopes)
3. Go to **Apps → voidtower → Edit** and set `VOIDWATCH_TOKEN`
4. Click **Save** — TrueNAS restarts Odysseus automatically
5. Open `http://<truenas-ip>:7000` → **Settings → Integrations → Voidwatch** → status should show green

> **Port note:** VoidTower uses `8443` (HTTPS) and `8080` (HTTP) to avoid conflicting with the TrueNAS web UI. Odysseus is on port `7000`.

---

## Option B — SSH Docker Compose

Full feature access including container management and self-update. Runs Docker directly on the TrueNAS host, bypassing k3s.

**1. SSH into TrueNAS**

```bash
ssh admin@<truenas-ip>
```

**2. Set your pool name**

```bash
export POOL=tank   # replace with your ZFS pool name — run: zpool list
```

**3. Clone and configure**

```bash
mkdir -p /mnt/$POOL/voidtower-app
curl -fsSL https://raw.githubusercontent.com/niwlekakan/voidtower/main/deploy/truenas/custom-app.yml \
  -o /mnt/$POOL/voidtower-app/custom-app.yml
curl -fsSL https://raw.githubusercontent.com/niwlekakan/voidtower/main/deploy/truenas/.env.example \
  -o /mnt/$POOL/voidtower-app/.env
nano /mnt/$POOL/voidtower-app/.env  # set ODYSSEUS_ADMIN_PASSWORD and TRUENAS_POOL=$POOL
```

**4. Start the stack**

```bash
TRUENAS_POOL=$POOL docker compose -f /mnt/$POOL/voidtower-app/custom-app.yml up -d
```

**5. First login and Voidwatch setup**

Same as Option A steps 6–7, but get the bootstrap token with:

```bash
docker logs voidtower
```

---

## Option B — Manual reinstall

Use this when you need a clean slate: new image, fresh data, fresh config.

```bash
export POOL=tank

# 1. Stop and remove containers
docker compose -f /mnt/$POOL/voidtower-app/custom-app.yml down
# Or by name if the compose file is gone:
docker rm -f voidtower chromadb searxng ntfy odysseus

# 2. Remove the old image
docker rmi ghcr.io/niwlekakan/voidtower:aio-latest

# 3. Wipe persistent data (skip to keep existing users, proxies, and settings)
rm -rf /mnt/$POOL/voidtower/data \
       /mnt/$POOL/voidtower/config

# 4. Pull the latest image and compose file
docker pull ghcr.io/niwlekakan/voidtower:aio-latest
mkdir -p /mnt/$POOL/voidtower-app
curl -fsSL https://raw.githubusercontent.com/niwlekakan/voidtower/main/deploy/truenas/custom-app.yml \
  -o /mnt/$POOL/voidtower-app/custom-app.yml

# 5. Start
TRUENAS_POOL=$POOL docker compose -f /mnt/$POOL/voidtower-app/custom-app.yml up -d

# 6. Get the bootstrap token
cat /mnt/$POOL/voidtower/config/bootstrap-token
```

Open `https://<truenas-ip>:8443` and enter the token to complete setup.

---

## Ollama on TrueNAS

Ollama is commented out in the YAML by default (large download, GPU passthrough requires extra config).

**Option A:** Edit the app YAML in the TrueNAS UI and uncomment the `ollama` service block, then save and restart.

**Option B:** Uncomment the `ollama` block in `deploy/truenas/custom-app.yml` and run `docker compose ... up -d` again.

For NVIDIA GPU passthrough, uncomment the `deploy.resources` block under `ollama` and ensure `nvidia-container-toolkit` is installed on the host. See [docs/gpu.md](../gpu.md) for details.

---

## Resetting the Odysseus password (Option A)

SSH into TrueNAS:

```bash
export POOL=tank
rm -f /mnt/$POOL/voidtower/odysseus/data/auth.json
# If permission error: docker exec odysseus rm -f /app/data/auth.json
```

Then go to **Apps → voidtower → Edit**, update `ODYSSEUS_ADMIN_PASSWORD`, and click **Save** — TrueNAS restarts Odysseus automatically. Log in at `http://<truenas-ip>:7000`.

## Resetting the Odysseus password (Option B)

```bash
export POOL=tank

# 1. Set the new password in .env
nano /mnt/$POOL/voidtower-app/.env
#    ODYSSEUS_ADMIN_PASSWORD=yournewpassword

# 2. Delete stored credentials so Odysseus recreates them on next start
rm -f /mnt/$POOL/voidtower/odysseus/data/auth.json
# If permission error: docker exec odysseus rm -f /app/data/auth.json

# 3. Restart Odysseus
docker compose -f /mnt/$POOL/voidtower-app/custom-app.yml restart odysseus
docker logs odysseus --tail 20
```

---

## Service management (Option B)

```bash
export POOL=tank

# Status
TRUENAS_POOL=$POOL docker compose -f /mnt/$POOL/voidtower-app/custom-app.yml ps

# Logs
docker logs -f voidtower
docker logs -f odysseus

# Restart a service
docker compose -f /mnt/$POOL/voidtower-app/custom-app.yml restart odysseus

# Stop all
docker compose -f /mnt/$POOL/voidtower-app/custom-app.yml down
```

For Option A, use the TrueNAS UI: **Apps → voidtower → Start / Stop / Restart**. Logs are at **Apps → voidtower → Logs**.

---

## Updates

**Option A:** TrueNAS shows an update banner when a newer image is available — click **Update** in **Apps → voidtower**. If no banner appears, edit the app YAML to reference the latest tag and save.

**Option B:**

```bash
export POOL=tank
docker pull ghcr.io/niwlekakan/voidtower:aio-latest
TRUENAS_POOL=$POOL docker compose -f /mnt/$POOL/voidtower-app/custom-app.yml up -d
```

---

## Uninstall

**Option A:**

1. Go to **Apps → voidtower → Delete**
2. To also wipe persistent data: `rm -rf /mnt/tank/voidtower` (replace `tank` with your pool name)

**Option B:**

```bash
export POOL=tank
docker compose -f /mnt/$POOL/voidtower-app/custom-app.yml down -v
rm -rf /mnt/$POOL/voidtower /mnt/$POOL/voidtower-app
```
