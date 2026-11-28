# Recovery & Maintenance

---

## Recovering admin access

Use this when you cannot log in to VoidTower — forgotten password, missing bootstrap token, or the setup wizard was never completed.

### Docker

If the bootstrap wizard was never completed, the token is still unconsumed:

```bash
docker exec voidtower voidtower --show-token
```

If setup was already completed and you have lost the admin password, reset it in place — this keeps the account, its role, and all other data intact:

```bash
docker exec voidtower voidtower user list
docker exec voidtower voidtower user reset-password --username <name> --password <newpassword>
```

The user is required to change the password again on next login.

As a last resort (e.g. the `users` table itself is corrupted), wipe the database and start fresh:

```bash
# Stop all services
docker compose --profile aio --profile ai down

# Delete only the database (other data in the volume is preserved)
docker run --rm -v voidtower-data:/data alpine rm -f /data/voidtower.db

# Start again
docker compose --profile aio up -d

# Retrieve the new bootstrap token
docker exec voidtower voidtower --show-token
```

Open the UI and complete the setup wizard. Files, proxies, and other non-auth data remain intact.

### Bare metal / LXC

If the token file still exists (setup was never completed):

```bash
sudo cat /etc/voidtower/bootstrap-token
# or:
sudo voidtower --show-token
```

If setup is complete and you are locked out, reset the password in place:

```bash
sudo voidtower user list
sudo voidtower user reset-password --username <name> --password <newpassword>
```

Only if that's not enough (e.g. you need to wipe everything and start over):

```bash
# Interactive — prompts for each item; answer yes to database and bootstrap token, no to everything else
sudo bash scripts/install.sh --reset

# Non-interactive full state wipe
sudo bash scripts/install.sh --reset --yes
```

After restart, a new bootstrap token is written to `/etc/voidtower/bootstrap-token`:

```bash
sudo voidtower --show-token
```

### TrueNAS Option A

SSH into TrueNAS:

```bash
export POOL=tank

# Read the token if setup was never completed
cat /mnt/$POOL/voidtower/config/bootstrap-token

# If locked out — reset the password in place (keeps the account and all data)
docker exec voidtower voidtower user list
docker exec voidtower voidtower user reset-password --username <name> --password <newpassword>

# Last resort — wipe the database to trigger a fresh setup wizard
rm -f /mnt/$POOL/voidtower/data/voidtower.db
```

Restart from **Apps → voidtower → Restart**, then find the new token in **Apps → voidtower → Logs (voidtower container)** (only needed if you wiped the database).

### TrueNAS Option B

```bash
export POOL=tank

# Read the token if setup was never completed
cat /mnt/$POOL/voidtower/config/bootstrap-token

# If locked out — reset the password in place
docker exec voidtower voidtower user list
docker exec voidtower voidtower user reset-password --username <name> --password <newpassword>

# Last resort — wipe the database
docker compose -f /mnt/$POOL/voidtower-app/custom-app.yml down
rm -f /mnt/$POOL/voidtower/data/voidtower.db
TRUENAS_POOL=$POOL docker compose -f /mnt/$POOL/voidtower-app/custom-app.yml up -d
docker logs voidtower 2>&1 | grep -i token
```

---

## Odysseus password reset

Use this when you have lost access to the Odysseus AI workspace.

### Docker

First, check whether the original configured password is still in the container environment:

```bash
docker exec odysseus env | grep ODYSSEUS_ADMIN_PASSWORD
```

If that returns a value and the password was never changed inside the Odysseus UI, use it — no restart needed.

If the password was changed in-app or the env var isn't set, reset it:

```bash
# 1. Update the password in .env
nano .env
#    ODYSSEUS_ADMIN_PASSWORD=yournewpassword

# 2. Delete stored credentials so Odysseus recreates them on next start
docker compose exec odysseus rm -f /app/data/auth.json

# 3. Restart Odysseus
docker compose restart odysseus

# 4. Confirm startup
docker compose logs odysseus --tail 20
```

Then log in at `http://localhost:7000` with the new password.

### Bare metal / LXC

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

### TrueNAS

See [docs/platforms/truenas.md](platforms/truenas.md#resetting-the-odysseus-password-option-a).

---

## Full reset (wipe data, keep binary)

Clears all users, proxies, secrets, and settings. The binary, service unit, and system users are preserved.

### Docker

```bash
docker compose --profile aio --profile ai down -v
docker compose --profile aio up -d
```

The `-v` flag removes all named volumes. Containers are recreated with fresh empty volumes.

### Bare metal / LXC

```bash
# Interactive
sudo bash scripts/install.sh --reset

# Non-interactive
sudo bash scripts/install.sh --reset --yes
```

After restart, the new bootstrap token is available via `sudo voidtower --show-token`.

### TrueNAS Option A

SSH into TrueNAS:

```bash
export POOL=tank
rm -rf /mnt/$POOL/voidtower/data /mnt/$POOL/voidtower/config
```

Restart from **Apps → voidtower → Restart**. VoidTower regenerates its config and bootstrap token on next start — find the token in the app logs.

### TrueNAS Option B

```bash
export POOL=tank
docker compose -f /mnt/$POOL/voidtower-app/custom-app.yml down
rm -rf /mnt/$POOL/voidtower/data /mnt/$POOL/voidtower/config
TRUENAS_POOL=$POOL docker compose -f /mnt/$POOL/voidtower-app/custom-app.yml up -d
docker logs voidtower 2>&1 | grep -i token
```

---

## Full reinstall (clean slate)

Removes everything — binary, data, config, and service — then starts over from scratch.

### Docker

```bash
docker compose --profile aio --profile ai down -v
docker rmi ghcr.io/niwlekakan/voidtower:aio-latest
docker compose --profile aio up -d
```

### Bare metal / LXC

```bash
sudo bash scripts/install.sh --uninstall --yes
curl -fsSL https://raw.githubusercontent.com/niwlekakan/voidtower/main/scripts/install.sh \
  | sudo bash -s -- --all-in-one
```

### TrueNAS Option A

1. Go to **Apps → voidtower → Delete**
2. SSH in and remove the dataset: `rm -rf /mnt/tank/voidtower`
3. Redeploy from **Apps → Discover Apps → Custom App** using the YAML from [`deploy/truenas/custom-app.yml`](../deploy/truenas/custom-app.yml)

### TrueNAS Option B

See [Manual reinstall](platforms/truenas.md#option-b--manual-reinstall).

---

## Repair (service won't start, wrong permissions)

Re-downloads the binary and reinstalls the service unit without touching any data or config.

### Docker

```bash
# Re-pull the image and force-recreate the container
docker compose --profile aio pull voidtower
docker compose --profile aio up -d --force-recreate voidtower
```

If Odysseus won't start:

```bash
docker compose logs odysseus --tail 50
docker compose restart odysseus
```

### Bare metal / LXC

```bash
sudo bash scripts/install.sh --repair
```

This re-downloads the binary, reinstalls the systemd service unit, fixes file ownership and permissions under `/opt/voidtower`, `/var/lib/voidtower`, and `/etc/voidtower`, then restarts the service. No data or config is changed.

---

## Database upgrades, pre-migration snapshots, and plaintext residue

VoidTower upgrades older databases automatically on first start. Two behaviors affect what is left on disk.

**Pre-migration snapshots.** When an untracked (pre-numbered-migration) database is adopted, the controller writes a full copy of the pre-migration file named `voidtower.db.pre-migration-v1-<timestamp>.bak` next to the database, then immediately restricts it to mode `0600` (it is created with the process umask and tightened straight after creation). Because it is the *pre-migration* file, it can still contain credential plaintext that the upgrade later encrypts or deletes, and it is also your only rollback point for that upgrade. Keep it until the new version has run correctly, then delete the single snapshot you no longer need:

```bash
ls -l /var/lib/voidtower/voidtower.db.pre-migration-v1-*.bak
sudo rm -f /var/lib/voidtower/voidtower.db.pre-migration-v1-<timestamp>.bak
```

Delete one named file rather than the whole glob, so you do not remove a rollback point you still want. Deleting a file is not a secure erase: on SSDs, copy-on-write filesystems, storage snapshots, and the Docker overlay used by the documented deployment, the old blocks may survive the unlink. If that matters for your threat model, keep the database on encrypted storage and treat the snapshot as sensitive for as long as it exists.

**Plaintext residue rebuild.** Deleting a row does not erase its bytes from the SQLite file. After the legacy credential migrations run, the controller rebuilds the database file once (`VACUUM` plus a truncating WAL checkpoint) so deleted credential values are gone from both the file and the write-ahead log. The rebuild:

- runs once per database, recorded in the `db.legacy_plaintext_residue_scrub` settings key;
- runs while holding the migration lock, so a second controller starting at the same moment waits for it;
- needs free space roughly equal to the database file and lengthens first start in proportion to database size;
- only affects deleted rows — a value still stored in a live column is copied as-is;
- writes the marker only after the checkpoint reports a complete rewrite. If it cannot finish — another process holds a read snapshot, the disk is full, and so on — the controller logs `plaintext residue rebuild deferred to the next startup`, leaves the marker unset, and retries on the next start.

There is no separate status field for the rebuild. To check whether it is still pending, look for the marker key; an empty result means the next start will retry:

```bash
sqlite3 /var/lib/voidtower/voidtower.db \
  "SELECT value FROM settings WHERE key = 'db.legacy_plaintext_residue_scrub'"
```

To check a database and log for provider plaintext rather than trusting the marker, run the read-only
scanner. The unit sends both output streams to the journal, so export the log to a file first (for a
Docker install use `docker logs voidtower > /tmp/voidtower.log 2>&1` instead). It reports a finding
kind, file name, pattern name, and — where one applies — a byte offset without printing the matched
value. It exits `0` only for a clean scan, `1` when findings exist, and `2` when no clean result can
be claimed (a missing, unreadable, non-regular or oversized input, a database with no readable
`settings(key, value)` table, a single match beyond the bounded carry, or a `--db`/`--log` that was
not both named):

```bash
journalctl -u voidtower --no-pager -o cat > /tmp/voidtower.log
python3 scripts/g5_plaintext_scan.py \
  --db /var/lib/voidtower/voidtower.db \
  --log /tmp/voidtower.log \
  --json
```

Byte patterns cover only values with a discoverable shape: OpenAI and Google keys, Slack tokens and
incoming-webhook URLs, Discord webhook URLs, Proxmox API tokens in the `user@realm!tokenid=uuid`
form, PEM private keys, and `Bearer` values. The ntfy topic URL and the Odysseus webhook secret have
no such shape. The live-row check reports them while their settings row exists, but a deleted copy is
covered only by the rebuild above — if either was deleted after the rebuild ran, a clean scan does
not prove it is gone. Every report lists those two classes under `coverage_limits` for that reason.

Findings of kind `snapshot` refer to a retained `*.pre-migration-v1-*.bak` file. Those are the
pre-migration database, so they can still hold values the upgrade later encrypted or deleted; decide
their retention explicitly rather than deleting them blindly. A pre-migration copy renamed outside
that pattern is not classified, so name it explicitly with an additional `--db` when you keep one.
The one-time rebuild does not cover plaintext deleted *after* it has run, and `secure_delete` is off,
so a finding of kind `residue` in a live database means a pattern-shaped plaintext value was deleted
after the rebuild — treat it as a real exposure.
