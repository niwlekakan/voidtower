# VoidTower on Proxmox LXC

Running VoidTower in a Proxmox LXC container is the recommended approach for Proxmox users — it gives you a clean isolated environment with direct Docker access and full container management.

---

## 1. Create the LXC

In the Proxmox web UI:

- **Template:** Ubuntu 22.04 or 24.04
- **RAM:** 2 GB minimum (4 GB recommended if running Odysseus + Ollama)
- **Disk:** 20 GB minimum on local-lvm or your preferred storage
- **CPU:** 2 cores minimum
- **Network:** DHCP or a static IP on your LAN bridge

> **Unprivileged containers:** Docker requires kernel features that are blocked in unprivileged LXC by default. Either:
> - Use a **privileged** container (simpler, fine for a homelab), or
> - Use unprivileged with these options set in the LXC config (`/etc/pve/lxc/<id>.conf`):
>   ```
>   features: nesting=1,keyctl=1
>   ```
>   then add:
>   ```
>   lxc.apparmor.profile: unconfined
>   lxc.cap.drop:
>   ```

## 2. Start and SSH in

```bash
ssh root@<lxc-ip>
```

## 3. Run the installer

Identical to any Ubuntu/Debian system — the installer handles Docker, systemd, and all dependencies automatically:

```bash
# VoidTower only
curl -fsSL https://raw.githubusercontent.com/niwlekakan/voidtower/voidtower-aio/scripts/install.sh \
  | sudo bash

# Full AIO stack with Odysseus and AI
curl -fsSL https://raw.githubusercontent.com/niwlekakan/voidtower/voidtower-aio/scripts/install.sh \
  | sudo bash -s -- --all-in-one --pull-model
```

The installer detects Ubuntu, installs Docker Engine via the official apt repository, enables the `docker` service, and starts VoidTower.

## 4. Access

Open `http://<lxc-ip>:8743` and complete the bootstrap. Credentials are saved to `/root/voidtower-bootstrap-token`.

If you ran `--all-in-one`, Odysseus is at `http://<lxc-ip>:7000` — credentials in `/root/odysseus-bootstrap-token`.

---

## Resetting the Odysseus password

```bash
# 1. Delete stored credentials
sudo rm -f /var/lib/odysseus/auth.json

# 2. Set the new password
sudo nano /opt/odysseus/.env
#    ODYSSEUS_ADMIN_PASSWORD=yournewpassword

# 3. Re-apply credentials
sudo -u odysseus bash -c "
  cd /opt/odysseus
  ODYSSEUS_ADMIN_USER=admin \
  ODYSSEUS_ADMIN_PASSWORD=yournewpassword \
  venv/bin/python setup.py
"

# 4. Restart
sudo systemctl restart odysseus
```

Then log in at `http://<host>:7000` with the new password.
