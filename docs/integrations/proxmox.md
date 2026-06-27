# Proxmox Integration

VoidTower connects to any Proxmox VE host via its API — the browser never talks to Proxmox directly. All requests are proxied through the VoidTower backend, tokens are stored encrypted, and VM state changes fire VoidTower alerts automatically.

> This doc covers connecting Proxmox to VoidTower's UI. For running VoidTower *inside* a Proxmox LXC container, see [docs/platforms/proxmox-lxc.md](../platforms/proxmox-lxc.md).

---

## Creating a Proxmox API token

VoidTower authenticates to Proxmox using API tokens, not your login credentials.

1. In the Proxmox web UI, go to **Datacenter → Permissions → API Tokens**
2. Click **Add**
3. Select the user (e.g. `root@pam` or a dedicated user)
4. Give the token a name (e.g. `voidtower`)
5. **Uncheck "Privilege Separation"** — this inherits the user's permissions rather than requiring explicit per-path grants
6. Click **Add** — copy the token secret immediately (shown once)

The token ID will look like `root@pam!voidtower`. The secret is a UUID.

### Minimum permissions (if using privilege separation)

If you prefer a least-privilege setup, create a dedicated user and role with at least:

| Permission path | Required for |
|---|---|
| `/` → `PVEAuditor` | Reading node status, VMs, storage |
| `/nodes` → `PVESysAdmin` | Node metrics, task log |
| `/vms` → `PVEVMAdmin` | VM start, stop, reboot, snapshot |

---

## Adding a host to VoidTower

Go to **VoidTower → VMs → Proxmox → Add Host** and fill in:

| Field | Value |
|---|---|
| **Name** | Display name (e.g. `homelab-pve`) |
| **URL** | Proxmox web UI URL (e.g. `https://192.168.1.10:8006`) |
| **Node** | Proxmox node name (default: `pve`) — find it in the Proxmox sidebar |
| **Token ID** | The full token ID: `user@realm!tokenname` (e.g. `root@pam!voidtower`) |
| **Token Secret** | The UUID secret shown when you created the token |
| **Fingerprint** | Optional TLS fingerprint — leave blank to accept any cert |

The token is stored encrypted in VoidTower's secrets table under `proxmox_token_{host_id}`. It is never exposed in API list responses.

Click **Test Connection** before saving — this verifies the URL and token are correct.

---

## What you can see

Once a host is connected, the **VMs → Proxmox** page shows:

- **Node overview** — CPU, RAM, disk usage, kernel version, subscription status (per node)
- **VM/LXC list** — all QEMU VMs and LXC containers across all nodes, with VMID, name, type, node, status, CPU, RAM, uptime
- **Storage** — all storage pools with type, capacity, used/available
- **Task log** — last 50 Proxmox tasks across all nodes

---

## VM actions

Select any VM or LXC container from the list to access:

| Action | Effect |
|---|---|
| **Start** | Power on the VM/container |
| **Stop** | Force stop (equivalent to pulling the power) |
| **Reboot** | Graceful reboot via QEMU guest agent / lxc-stop |
| **Snapshot** | Creates a new snapshot — prompts for snapshot name via change-plan modal |
| **Rollback** | Rolls back to a selected snapshot — shown in the change-plan modal with risk: high |

All lifecycle actions require admin or owner role. Snapshot and rollback show a dry-run plan before executing.

---

## noVNC console

Click the **Console** button on any running QEMU VM to open a noVNC session. The browser connects directly to the Proxmox `vncwebsocket` endpoint on your LAN — VoidTower does not proxy the WebSocket traffic.

Requirements:
- The Proxmox host must be reachable from the browser (LAN or VPN)
- QEMU guest agent is recommended for clipboard support
- CPR (`\x1b[6n`) cursor position queries are handled by the frontend

LXC containers do not have a noVNC console — use the VoidTower terminal or SSH instead.

---

## PBS backup tab

The **Backups** tab on the Proxmox page shows:

- Cluster-level scheduled backup jobs (from `/cluster/backup`)
- Backup archives on each node's storage pools that contain backup content

This is read-only — VoidTower does not create or delete PBS backup jobs. To manage PBS schedules, use the Proxmox UI directly.

---

## VM state monitor

VoidTower polls all connected Proxmox hosts every 90 seconds in the background. When a VM or LXC transitions state, a VoidTower alert is automatically created:

| Transition | Severity |
|---|---|
| `running` → anything | `warning` — "VM stopped: {name}" |
| anything → `running` | `info` — "VM started: {name}" |
| any other change | `info` — "VM state changed: {name}" |

Alerts appear in **VoidTower → Alerts** and are included in the SSE event stream for Voidwatch.

---

## Deploy to Proxmox LXC from App Vault

App Vault apps can be deployed directly to a Proxmox LXC container instead of the local Docker daemon. On the App Vault deploy modal, select **Deploy target → Proxmox LXC**, choose a host and storage pool, and VoidTower will:

1. Create a new LXC container on the selected Proxmox node
2. Run a bootstrap script inside it to install Docker and the app
3. Return the container's IP once it's running

---

## Multiple hosts

VoidTower supports multiple Proxmox hosts. Add each one separately — they appear as tabs or a host selector in the VMs page. The VM state monitor polls all hosts.

---

## Removing a host

Go to **VoidTower → VMs → Proxmox → (host) → Remove**. This deletes the host record and its encrypted token from the database. No changes are made to Proxmox.
