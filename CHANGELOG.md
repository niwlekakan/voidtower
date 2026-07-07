# Changelog

All notable changes to VoidTower are documented in this file.

Format follows [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).
Versioning is [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [Unreleased]

### Added

- **Multi-provider AI orchestrator** (`backend/src/ai/`) — VoidTower no longer depends on Odysseus as its sole AI backend. A new `AiProvider` trait abstracts chat completions and streaming; four adapters ship out of the box: `odysseus` (unchanged, HTTP-only), `openai`, `anthropic` (Claude), and `local` (Ollama / llama.cpp via OpenAI-compat endpoint). Providers are persisted in a new `ai_providers` DB table and managed at **Settings → AI Providers** (`/ai-providers`). The lowest-priority enabled provider is selected automatically; individual requests can pin a provider via `provider_id`. The existing `odysseus.allowed_url` settings key continues to work as a zero-config fallback for existing installs.
- **`/api/ai/providers` endpoints** — `GET`, `POST`, `PUT /:id`, `DELETE /:id`, `GET /:id/health` for full provider lifecycle management including connectivity tests.
- **Ask popup provider selector** (`AiosAskPopup.tsx`) — dropdown lists all enabled providers; "Auto" uses priority order; selection is per-conversation.
- **MCP server expansion** (`odysseus-mcp-servers/`) — `voidtower_server.py` expanded from 10 to 38 tools, adding proxy management, firewall rules, VM lifecycle, app deploy/undeploy, service logs, status checks, tags, and the full App Vault lifecycle. 39 new app-specific MCP servers ship alongside it — one per App Vault catalog app (sonarr, radarr, lidarr, readarr, prowlarr, bazarr, jellyseerr, jellyfin, immich, navidrome, kavita, freshrss, qbittorrent, minio, outline, paperless, mealie, vikunja, n8n, homeassistant, matrix-synapse, gitea, changedetection, ollama, llama-cpp, open-webui, comfyui, portainer, authentik, nextcloud, vaultwarden, searxng, stirling-pdf, pihole, adguardhome, uptime-kuma, grafana, syncthing, wireguard-easy, tailscale) — ~413 tools total. Each server is a standalone Python process registered in Odysseus or any MCP client independently; see `docs/integrations/mcp-server.md`.
- **App Vault — AnythingLLM** (`app-vault/apps/anythingllm.yml`) — self-hosted AI assistant with RAG, agents, multi-user workspaces, and native MCP tool support. Single container on port 3001; auto-generates JWT secret; connects to Ollama via `vt-proxy` network. Exposes an OpenAI-compatible API at `http://anythingllm:3001/api/openai` for VoidTower AI provider config.
- **App Vault — LibreChat** (`app-vault/apps/librechat.yml`) — multi-provider AI chat with OpenAI, Anthropic, Ollama, and built-in MCP client support. Three-service stack (librechat + MongoDB 7 + Meilisearch v1.7); port 3080; five auto-generated secrets. Configure VoidTower AI providers as custom OpenAI-compatible endpoints in the LibreChat admin panel.

### Changed

- `POST /api/ai/ask` now routes through the multi-provider orchestrator instead of directly calling Odysseus. Accepts an optional `provider_id` field to pin a specific provider for that request.
- **Installer — AI wrapper selection moved to App Vault** — the interactive "Install Odysseus AI workspace?" prompt (`offer_odysseus`) has been removed from `scripts/install.sh`. AI wrapper selection (Odysseus, Open WebUI, AnythingLLM, LibreChat) is now handled entirely through App Vault after VoidTower is running, keeping the install flow lean and wrapper-agnostic.

---

- **App Vault — pre-deploy configuration modal** — clicking Deploy on a catalog app now opens a modal showing the full compose YAML, env var overrides, and any required secrets before deployment starts. After deploy, the modal streams the `docker compose up` log output so you can see exactly what happened.
- **App Vault — `required_env` catalog field** — app definitions can declare required environment variables with an optional `generate` strategy (`random_hex_16`, `random_hex_24`, `random_hex_32`, `random_hex_50`, `random_hex_64`, `uuid`). Auto-generated secrets are injected at deploy time with no user input; manually required fields are shown as validated inputs in the pre-deploy modal.
- **App Vault — Euro-Office** — new catalog entry for [Euro-Office DocumentServer](https://github.com/Euro-Office) (`ghcr.io/euro-office/documentserver:latest`). Collaborative DOCX/XLSX/PPTX editing server; `EUROOFFICE_JWT_SECRET` auto-generated on deploy.
- **App Vault — security audit (all catalog apps)** — full pass over every app definition to replace hardcoded secrets and default passwords with `${VAR}` references backed by `required_env` entries. Apps updated:
  - **Authentik** — `AUTHENTIK_SECRET_KEY` (random_hex_50) and `AUTHENTIK_POSTGRES_PASSWORD` (random_hex_32) replace four `changeme` literals
  - **Immich** — `IMMICH_DB_PASSWORD` (random_hex_32) replaces hardcoded `immich` DB password
  - **Outline** — `OUTLINE_SECRET_KEY` (random_hex_64), `OUTLINE_UTILS_SECRET` (random_hex_64), `OUTLINE_DB_PASSWORD` (random_hex_32)
  - **n8n** — `N8N_PASSWORD` (random_hex_16) replaces `changeme` basic-auth password
  - **MinIO** — `MINIO_ROOT_PASSWORD` (random_hex_16) replaces `changeme123`
  - **Paperless-ngx** — `PAPERLESS_SECRET_KEY` (random_hex_50) replaces placeholder string
  - **Open WebUI** — `WEBUI_SECRET_KEY` (random_hex_32); removed insecure `:-changeme` default fallback
  - **Nextcloud** — added `NEXTCLOUD_ADMIN_USER=admin`, `NEXTCLOUD_ADMIN_PASSWORD` (random_hex_24), expanded trusted domains
  - **Matrix Synapse** — `MATRIX_SERVER_NAME` (user-provided, default `matrix.home`) replaces hardcoded server name
  - **Gitea** — `GITEA_SECRET_KEY` and `GITEA_INTERNAL_TOKEN` (both random_hex_64) added to security config
  - **Vikunja** — `VIKUNJA_JWT_SECRET` (random_hex_32) replaces `changeme-random-secret`
  - **code-server**, **Grafana**, **Vaultwarden**, **Pihole**, **WireGuard Easy**, **Gluetun**, **SearXNG**, **Tailscale** — secrets parameterised and `required_env` added
- **App Vault — Jitsi Meet rewrite** — added missing `prosody` XMPP service (jicofo and jvb silently failed auth without it). `JITSI_JICOFO_PASSWORD`, `JITSI_JICOFO_COMPONENT_SECRET`, and `JITSI_JVB_PASSWORD` auto-generated and wired consistently across all four services.
- **OpenCloud — working AIO config** — switched to `opencloud init` entrypoint pattern: on first start, `init` generates `/etc/opencloud/opencloud.yaml` with all internally-consistent service passwords before `server` starts. Eliminates the LDAP Result Code 49 "Invalid Credentials" error that occurred when `IDP_LDAP_BIND_PASSWORD` and `IDM_IDPSVC_PASSWORD` were set independently.
- **OpenCloud — nginx-proxy support via `PROXY_TLS=false`** — OpenCloud's internal proxy now serves plain HTTP on port 9200 (`PROXY_TLS=false`), allowing nginx-proxy to connect without upstream TLS. `OC_URL` remains `https://opencloud.local` so the IDP continues to issue HTTPS-issuer tokens. Previously nginx-proxy rejected the upstream self-signed cert (`tls: bad certificate`), making OpenCloud inaccessible through the proxy.
- **Authentik SSO — central identity for VoidTower and App Vault apps** — Authentik can now be VoidTower's platform-wide identity provider. New `oidc_config` table and `backend/src/oidc.rs` back a "Login with Authentik" OIDC client (Settings → Authentik SSO: issuer URL, client ID/secret, redirect URL, scopes, and a group→role mapping that's re-evaluated on every login). Local username/password (+ TOTP) login is unaffected — SSO is additive. Proxy rules gained a **"Protect with Authentik"** toggle (off by default) that fronts the proxy with Authentik's embedded outpost in forward-auth mode, requiring login (and MFA, if configured) before traffic reaches the app — no second container needed. See [`docs/integrations/authentik-sso.md`](docs/integrations/authentik-sso.md).
- **App Vault — `required_env` fields now support a `default`** — entries with a `default` (e.g. Matrix's `MATRIX_SERVER_NAME`, Authentik's `AUTHENTIK_BOOTSTRAP_EMAIL`) are pre-filled in the deploy modal instead of needing to be typed in blank, and only block deploy if actually emptied out.
- **App Vault — generated secrets are now shown after deploy** — auto-generated `required_env` values (DB passwords, admin tokens, JWT secrets) were resolved at deploy time but never surfaced anywhere, making them unrecoverable for anything credential-like. The deploy response now includes them, and the modal's success screen shows a "Generated secrets" box.
- **App Vault — `post_deploy` catalog hook** — apps can declare a one-shot command (with `${VAR}` substitution from the same resolved required_env/overrides) to run via `docker exec` in the background after a successful deploy, retried every 3s until it succeeds or a timeout elapses. Used by Authentik, whose `AUTHENTIK_BOOTSTRAP_PASSWORD`/`AUTHENTIK_BOOTSTRAP_EMAIL` env vars don't reliably apply (a migration seeds a default `akadmin` user with no usable password before the bootstrap check runs) — the hook sets the admin account directly via `ak shell` instead, so the generated credentials in the deploy modal actually work without a manual recovery-link workaround.
- **App Vault — Logs/Terminal access for deployed containers** — each container row in a deployment's "Containers" tab now links to the same `/containers/:id` page (live log streaming + interactive exec terminal) already used by the standalone Containers tab, instead of only offering a static one-shot log fetch.
- **Theming — hover animation levels** — new "Hover animations" setting (Settings → Interface) with four levels: Off, Subtle, Normal, Playful. Drives a `data-hoverfx` attribute on `<body>` that scopes distinct hover motion per element family (buttons lift/brighten, clickable cards lift + glow, sidebar nav items slide with a growing accent bar, removable tag pills scale/tilt, clickable table rows get a background sweep). Respects the existing `a11y-reduce-motion` setting, which still collapses all transition/animation durations to ~0 regardless of the chosen level.

### Fixed

- **Security — viewer role could remove containers and open a shell inside them**: `POST /api/containers/:id/action` and `GET /api/containers/:id/exec` only checked that a session existed, not its role, so any logged-in `viewer` account could stop/restart/**remove** any Docker container or open an interactive `docker exec -it <id> sh` PTY session. Every sibling handler (`services::action`, `containers::propose_compose/apply_compose`) already rejected `role == "viewer"` — both handlers now do too.
- **Security — Timeline search/filter built SQL via string interpolation**: `GET /api/timeline`'s `outcome`/`search` query params were concatenated into the `WHERE` clause with manual quote-escaping instead of bound parameters. Not exploitable as shipped, but a landmine for the next field added without the same escaping. Rebuilt with `sqlx::QueryBuilder` and `.push_bind()` throughout.
- **Security — Odysseus webhook secret comparison wasn't constant-time**: despite a comment claiming otherwise, the check was a plain `String !=` on SHA-256 hashes, which short-circuits on the first differing byte. Replaced with a real constant-time byte comparison.
- **Frontend — `npm run lint` was silently enforcing zero rules**: `frontend/eslint.config.js` had been accidentally emptied by a prior commit (`5e03594`, which intended to *register* the `eslint-plugin-react-hooks` plugin but wiped the file instead), so ESLint applied no config to any source file — `react-hooks/rules-of-hooks` and `exhaustive-deps` weren't running despite being listed as devDependencies. Restored the config plus the react-hooks plugin registration, and excluded the untracked `ds-bundle/` vendor directory that was throwing unrelated "rule not found" errors.
- **AIOS — panel could crash with "Rendered fewer hooks than expected"**: `AiosPanel.tsx` declared 10 `useCallback` hooks *after* early returns for the phone/TV device tiers — a rules-of-hooks violation invisible while lint was broken (see above). If a panel's device tier changed at runtime (e.g. resizing across the phone/tablet breakpoint), React would throw and crash the panel. All affected hooks moved above the early returns; a stale-closure bug surfaced in the same pass (the titlebar drag handler wasn't re-created when `restorePanel`/`panel.layoutMode` changed) is fixed alongside it.
- **AIOS — status bar height drift**: `AiosKioskLayout.tsx` locally redeclared `STATUS_BAR_H = 28` instead of importing the canonical constant, and the Zustand store's default `dims.statusH` was hardcoded to `36` instead of `28` — both silently correct today but one bad edit away from a visible layout offset. Kiosk layout now imports the constant; the store's default is corrected with a comment explaining why it isn't imported directly (would create a circular dependency with `AiosStatusBar.tsx`). Also fixed: the dims-sync effect and `openApp`/`openOdysseus` in `AiosLayout.tsx` were missing `dockLeft`/`statusBarH`/`dockH` from their dependency arrays, so panel geometry could use stale values after a dock-position or bar-height change.
- **Updates page bypassed the dry-run/ChangePlan pattern for every destructive action**: self-update (Docker and git modes), rollback, OS package updates, and per-container image updates all either used a raw `confirm()` string or — for per-container updates — no confirmation at all, instead of the structured `ChangePlanModal` preview used elsewhere (proxy, firewall, container remove, backups, Proxmox). `apply_vt`, `rollback_vt`, and `docker_apply` now accept a `dry_run` flag and return a real change plan (image/commits-behind/risk for self-update, target tag for rollback, container/image for per-container updates); `apply_os`'s dry-run response is reshaped from raw simulated command output into the same `{dry_run, plan}` shape, backed by a `list_upgradable_packages()` helper shared with the OS panel's package list so the two can never disagree on count. All five flows in `Updates.tsx` now show the plan before executing.

- **Installer — CLI management commands not on PATH after bare-metal install**: `voidtower user list`, `voidtower backup run`, and the other CLI management subcommands failed with a path error on bare-metal/systemd installs (including TrueNAS installs using `install.sh --musl`) because the binary lives at `/opt/voidtower/voidtower` but nothing placed it in PATH. The installer now creates a `/usr/local/bin/voidtower → /opt/voidtower/voidtower` symlink on fresh install, `--update`, and `--repair`; `--uninstall` removes it. Docker installs are unaffected — the Dockerfile already copies the binary directly to `/usr/local/bin/voidtower` inside the image, and CLI commands work there via `docker exec -it voidtower voidtower <subcommand>`.

- **App Vault — `required_env` secrets never reached containers**: Auto-generated/user-supplied `required_env` values (Authentik, Matrix Synapse, Jitsi, Outline, Euro-Office, Vaultwarden, etc.) were injected as literal `KEY=value` entries into the compose `environment:` array, but every catalog template references them via `${KEY}` shell-style interpolation — which `docker compose` only resolves from a `.env` file or its own process environment, never from another entry in the same array. Every deploy silently produced blank secrets (`"VAR" variable is not set. Defaulting to a blank string.`), breaking Postgres init, Matrix's `server_name`, Jitsi's XMPP auth, and Outline's `SECRET_KEY`/DB password. Resolved `required_env` values (generated, defaulted, or user-supplied) are now written to a `.env` file next to each app's `docker-compose.yml`, which `docker compose` picks up automatically. Also fixed: only `random_hex_64`/`uuid` generation strategies were actually implemented — `random_hex_16/24/32/50` silently produced a 32-char value regardless of the requested length; generation is now generic over `random_hex_N`. Vaultwarden's `required_env` entry for `VAULTWARDEN_ADMIN_TOKEN` (referenced in its compose file via `${VAULTWARDEN_ADMIN_TOKEN}`) had also gone missing from the catalog YAML — re-added.
- **App Vault — Redroid (Android VM) unreachable from the web UI**: No `web_port` was set, so the embed proxy defaulted to the first port found in the compose file — Redroid's raw ADB port `5555`, not scrcpy-web's noVNC HTTP port. Added `web_port: 6080` to target the actual browser-reachable service.
- **App Vault — interrupting a deploy could corrupt the host's Docker content store**: `docker compose` child processes inherited VoidTower's terminal foreground process group, so killing/restarting the dev server (e.g. Ctrl+C) sent the same signal straight to an in-flight `docker compose up`/`pull`, severing the daemon connection mid-layer-write — observed as `operation not permitted` errors reading specific blobs out of `/var/lib/containerd` on later, unrelated deploys. All `docker compose` subprocesses now spawn in their own process group (`process_group(0)`), so they're no longer killed by signals sent to VoidTower's process group and finish cleanly even across a dev-server restart. Deploys can now also be cancelled safely and intentionally: a new "Cancel deployment" button in the deploy modal calls `POST /api/apps/deploy/cancel/{project_name}`, which sends SIGTERM to the tracked `docker compose` pid and escalates to SIGKILL only if it hasn't exited after 5s — the supported, graceful cancellation path instead of an abrupt kill.
- **App Vault — YoukiDex port conflict with Gluetun**: YoukiDex (scrcpy-web) was mapped to host port `8888`, colliding with Gluetun's HTTP proxy port (`8888:8888`). YoukiDex moved to `8890:6080`.
- **Containers — detail page couldn't be opened by container name**: `/containers/:id` only matched on Docker ID or short ID, so linking to it by container name (as the new App Vault logs/terminal links do) hit "Container not found" even though the underlying log-stream/exec endpoints accept a name just fine. Now matches by name too.
- **Docs — Euro-Office ↔ OpenCloud integration**: added [`docs/integrations/opencloud-eurooffice.md`](docs/integrations/opencloud-eurooffice.md) covering OpenCloud's bundled `collaboration` (WOPI bridge) service config, verified directly against the `opencloudeu/opencloud` binary's compiled-in config struct rather than guessed.

- **App Vault — Proxmox deploy modal invisible**: The modal box used `var(--bg-surface)` which is not defined in any theme, rendering it fully transparent. Changed to `var(--bg-card)`.
- **App Vault — Deployed tab is now the default**: Deployed apps tab is shown first; Catalog is second.
- **App Vault — GGUF download silently accepted HTML**: Pasting a HuggingFace model page URL instead of a direct `.gguf` file URL returned an instant "Done" while saving an HTML file to disk. The download handler now rejects HTML `Content-Type` responses with a descriptive error, and validates GGUF magic bytes (`GGUF` header) after the download completes — non-GGUF files are deleted and the error surfaced. The URL input shows an inline warning when a model page URL is detected.
- **App Vault — GGUF download failing on HuggingFace CDN**: Requests without a `User-Agent` header were being rejected. Downloader now sends a standard User-Agent.
- **Models — llama.cpp active model always shown as unloaded**: Active model detection read the `command` field from the saved compose file, but `llama-cpp.yml` uses `entrypoint`. Detection now queries the live llama.cpp server's `/v1/models` API (port 8090 for Docker, 8080 for native) instead.
- **Models — Docker-deployed llama.cpp not auto-detected**: `detect_llm_endpoint()` only probed port 8080 (native llama.cpp). The Docker catalog entry maps to host port 8090, so it was never detected for `LLM_API_BASE` injection. Port 8090 is now probed first.

- **Odysseus themes** — 16 built-in themes ported from Odysseus's preset palette (Dark, Light, Midnight, Paper, Cyberpunk, Retrowave, Forest, Ocean, Ume, Copper, Terminal, Organs, Lavender, GPT, Claude, Cute). Available in Themes settings under a dedicated "Odysseus" section.
- **Sync from Odysseus** — "Sync from Odysseus" button on the Themes page reads Odysseus's active theme via `/api/integrations/odysseus/theme` and applies the matching VoidTower preset.
- **GPU controls in TopBar** — GPU widget (VRAM %, llama.cpp process list, Unload button) moved from the AI workspace overlay into the TopBar, sitting next to the Void Mode toggle. Always visible regardless of current page.
- **Void Mode toggle** — Void Mode button redesigned as a pill-style on/off toggle (label + sliding knob) to communicate its experimental nature; compact icon-only variant used in the AIOS status bar.
- **Sidebar collapse animations** — Tower Mode sidebar now supports 6 selectable collapse/expand animation styles (Slide & Fade, Simple Fade, Spring Squeeze, Staggered Reveal, 3D Flip, Bouncy Drop). Configurable from Settings → Navigation.

### Fixed

- **TrueNAS — App Vault deploys fail with `ParseAddr(".../64")`**: On hosts whose Docker daemon has an IPv6 default address-pool (e.g. TrueNAS SCALE, `fdd0::/48`), `docker compose up` failed for **every** app with `ParseAddr("fdd0:0:0:2::1/64"): unexpected character, want colon`. The compose-managed `default` network and the shared `vt-proxy` network were inheriting an auto-assigned IPv6 gateway whose stored CIDR suffix `docker compose` can't parse. All generated compose files now pin `enable_ipv6: false` on every non-external network, and `vt-proxy` is created with `--ipv6=false`. Existing installs with an already-broken `vt-proxy` network self-heal automatically (recreated IPv4-only, containers reconnected) — no manual `docker network rm vt-proxy` needed.
- **TrueNAS — App Vault bind-mounts resolve to the wrong host path**: When VoidTower itself runs containerized (TrueNAS SCALE Custom App), `/var/lib/voidtower` inside VoidTower's container is bind-mounted from `/mnt/<pool>/voidtower/data` on the host. Compose files VoidTower writes for other apps (e.g. nginx-proxy's `conf.d` bind mount) used `/var/lib/voidtower/...` as the bind-mount source — but the host's Docker daemon resolves that against its own root filesystem, not the mounted dataset, so the proxy never saw VoidTower-written configs. New `VOIDTOWER_HOST_DATA_DIR` env var (set in `deploy/truenas/custom-app.yml`) tells VoidTower the real host path; bind-mount sources under `data_dir` are rewritten to it before writing compose files. No-op on bare-metal installs.
- **TrueNAS — nginx-proxy / qbittorrent port collisions**: Both published host port `8080` (and nginx-proxy also `8443`), colliding with the VoidTower AIO container's own bundled proxy. nginx-proxy now publishes `8070`/`8453`; qbittorrent's WebUI publishes `8780` (container port unchanged).
- **TrueNAS — GPU apps (Ollama, etc.) never detected as GPU-capable**: `detect_gpu()` only checked for a local `nvidia-smi` binary, which is always absent inside the VoidTower AIO container even when the TrueNAS host has `nvidia-container-toolkit` configured (GPU assigned to apps in System Settings → Advanced). This caused `strip_gpu_requirements()` to always strip `runtime: nvidia` etc., so Ollama and other GPU apps deployed CPU-only on TrueNAS regardless of host GPU availability. `detect_gpu()` is now async and additionally checks `docker info --format '{{json .Runtimes}}'` (via the bind-mounted host Docker socket) for a registered `nvidia` runtime — true whenever the host's nvidia-container-toolkit is configured, independent of whether VoidTower's own container has GPU passthrough. No-op on bare-metal (local `nvidia-smi` check still applies first).
- **TrueNAS — Ollama deploy fails on missing `/dev/dri`**: Ollama's compose file mounts `/dev/dri:/dev/dri` for optional Intel/AMD VAAPI passthrough, which doesn't exist on NVIDIA-only or virtualised hosts (most TrueNAS GPU setups) and made `docker compose up` fail outright. New `strip_unavailable_devices()` removes any `devices:` entry whose host path doesn't exist before deploy, applied to all three deploy paths. NVIDIA access via `runtime: nvidia` is unaffected — only raw `/dev/*` device-node mounts are checked.
- **TrueNAS — Ollama model directory bind-mounts to the wrong host path**: Ollama's compose file bind-mounts `${HOME}/.local/share/voidtower/models` for shared model storage. `${HOME}` is expanded by `docker compose` using *VoidTower's own* environment — on the TrueNAS AIO that's the container's home (e.g. `/root`), which the host daemon then resolves against its own root filesystem, not VoidTower's data. New `rewrite_voidtower_home_paths()` rewrites any `${HOME}/.local/share/voidtower/...` bind-mount source to `data_dir/...` before `rewrite_host_bind_mounts()` maps it to the real host path. No-op on bare-metal (same directory either way).
- **AI integration — proxy conf path**: AI proxy nginx config was being written to `/etc/nginx/conf.d/` (system nginx, read-only) instead of `/var/lib/voidtower/nginx/conf.d/` (Docker nginx bind-mount). Saving the Odysseus URL in Settings → Integrations now works without error.
- **AI integration — port not published**: The AI proxy `listen {port}` directive was never exposed from the nginx-proxy Docker container (only 8080:80 and 8443:443 were published). When the AI port changes, the nginx-proxy `docker-compose.yml` is now patched to add the port binding and the container is redeployed. Saves with an unchanged port only reload nginx.
- **AI integration — 502 bad gateway**: `proxy_pass http://localhost:{port}` inside the nginx container targeted the container itself, not the host. The upstream URL is now rewritten to the `host.docker.internal` hostname before the conf is written, resolved inside nginx-proxy's own container via a `host-gateway` `extra_hosts` entry on its App Vault compose template (`app-vault/apps/nginx-proxy.yml`) — this works whether VoidTower itself is installed bare-metal or in Docker, unlike an earlier version of this fix that guessed a `docker0`-bridge IP from wherever VoidTower's own backend process happened to run (nginx-proxy is never actually attached to the default `docker0` bridge, only to its own custom networks, so that guess wasn't reliably reachable from inside it).
- **Proxy Manager / App Vault embed proxies — 502 bad gateway**: Same localhost-in-Docker issue applied to all nginx conf writers. `write_nginx_conf` (Proxy Manager domain rules) and `write_nginx_port_conf` (App Vault embed proxies) now apply the same `host.docker.internal` rewrite via the shared `rewrite_upstream_for_docker()` in `proxy.rs`. `docker_host_ip()` (best-effort bridge-IP guessing) still exists but is now only used by the on-demand proxy health check, which connects directly from VoidTower's own process rather than from inside nginx-proxy.
- **AI integration — port published but unreachable remotely**: The AI proxy port was added to nginx-proxy's Docker port bindings but never opened in the *host* firewall (unlike App Vault's embed-port feature, which already did this for its own ports). `set_ai_url` now opens the port in ufw/firewalld/iptables (via a `open_firewall_port()` shared with the embed-port feature, moved into `proxy.rs`) on every save, and closes the old one via the new `close_firewall_port()` counterpart when the port changes or the AI URL is cleared.
- **AI integration — iframe blocked as mixed content over HTTPS**: The AI tab's iframe always loaded a plain `http://` URL, which browsers block when VoidTower itself is reached over `https://`. `write_ai_proxy_conf` now also writes an HTTPS listener one port above the HTTP one, with a self-signed cert generated once into the conf.d bind-mount, and the frontend (`AppLayout.tsx`'s `PersistentAIFrame`, `Settings.tsx`) picks `http`/`https` and the matching port based on the page's own protocol. Since the cert is self-signed, the first HTTPS load may need a one-time manual visit to `https://<host>:<tls_port>/` to accept the browser's certificate warning.

- **Proxmox — storage uploads no longer buffer the entire file into RAM**: `upload_storage_content` previously read the whole multipart body into a `Vec<u8>` before forwarding to Proxmox, causing unbounded memory growth for large ISOs and VHDs. The handler now streams the request body directly to Proxmox via `reqwest::Body::wrap_stream` — memory use is constant regardless of file size.
- **Proxmox — audit entries now only fire on confirmed API success**: `vm_start`, `vm_reboot`, `vm_reset`, `vm_suspend`, and `vm_resume` wrote audit log entries before sending the request to Proxmox, so failed actions were indistinguishable from successful ones in the Global Timeline. Audit logging now happens inside the success branch only.
- **Proxmox — disk SMART query parameter is now URL-encoded**: The `/disks/smart?disk=` URL was built via string interpolation, meaning disk paths with special characters (e.g. device-mapper paths, colons) could corrupt the query string. The URL is now built with `reqwest::Url::parse_with_params`, which percent-encodes the value correctly.
- **Proxmox — poll interval reduced from 5 s to 10 s**: Both the AIOS native panel and the full Proxmox page polled the backend every 5 s — enough to generate noticeable background traffic on busy hosts. Doubled to 10 s.
- **Proxmox — disk passthrough bus slot validated before submitting**: `PassthroughModal` accepted any free-text string for the bus slot and forwarded it to Proxmox, which returned a cryptic API error for malformed values. Input is now validated against `/^(scsi|ide|sata|virtio)\d+$/` with a user-visible error before the change plan is fetched.
- **Proxmox — storage pools without a node assignment now show a visible indicator**: Pools whose `node` field is absent silently ignored click-to-expand, showing an empty panel with no explanation. The row now shows a `(no node)` label and a tooltip clarifying that content browsing is unavailable for that pool.

### Changed

- **nginx — Docker-only**: nginx is now exclusively managed through the App Vault nginx-proxy container. System nginx install, sudoers setup, and all system nginx fallback paths have been removed from the installer and backend. Deploy `nginx-proxy` from App Vault before using the Proxy Manager or embed proxy.

---

## [0.9.0] — 2026-06-08

First public release milestone. All planned must-have features are complete.

### Added — Core platform

- **Installer** (`scripts/install.sh`) — interactive, four install paths: bare-metal/LXC, Docker, TrueNAS Option A (Compose), TrueNAS Option B (Custom App + SSH). Auto-detects and installs Docker Engine on fresh systems. Opens firewall ports automatically (ufw → firewalld → iptables fallback). Shows LAN IP in post-install summary.
- **Proxmox LXC installer** — dedicated `install-lxc.sh` for deploying VoidTower as a Proxmox container.
- **Bootstrap token** — re-logged on every restart until first admin account is created; visible via `--show-token`.
- **Doctor / diagnostics** — `--doctor [--json]` CLI flag; mirrors `/api/diagnostics` checks; exits with code 1 on failures.
- **Disaster recovery** — `--export-config` / `--import-config` CLI flags; emergency admin reset and instance disable endpoints; UI page at `/disaster`.
- **Recovery & Maintenance** docs — README covers admin access recovery, Odysseus password reset (all 4 install paths), full reset, full reinstall, repair.

### Added — Security

- **Policy engine** — DB-backed rules that govern what automated actors (API tokens, automations) may do. Additive restrictions on top of token scopes. Default deny rules seeded on install: `ai-no-touch` resources, `critical` resources, API token `remove` actions. CRUD API + UI at `/policy`.
- **Secrets manager** — AES-256-GCM encrypted secrets table. Reveal, rotate (version counter), per-token secret scope restriction, disaster-recovery export/import.
- **TOTP 2FA** — setup/enable/disable in Security page; enforced at login.
- **Rate limiting** — failed login lockout with per-IP tracking.
- **Security headers** — `X-Frame-Options`, `X-Content-Type-Options`, `Referrer-Policy`, `Content-Security-Policy` on all responses.
- **SSRF guard** — blocks server-side requests to private/loopback ranges.
- **Session management** — active session list with revoke-others in Security page.
- **Audit log** — all state-changing operations recorded with actor, action, source.
- **API tokens** — scoped (`read`, `write:containers`, etc.), bearer auth with automatic session injection, per-token secret restriction.

### Added — Proxmox integration (Phase 3)

- **Multi-host management** — add/remove Proxmox hosts; tokens stored encrypted.
- **Read-only views** — nodes, VMs, LXCs, storage, task log, PBS backup jobs.
- **Lifecycle actions** — start, stop, reboot, shutdown, snapshot, rollback, delete snapshot. Dry-run / change-plan guard on destructive actions.
- **noVNC console** — in-browser VM console via Proxmox vncwebsocket; no local noVNC install needed (CDN).
- **VM alerts** — 90-second poll loop; fires alerts on unexpected state transitions.
- **Resource tags** — tag Proxmox VMs alongside containers, services, apps, proxies, backups.
- **PBS backup tab** — lists backup jobs and recent archives per host.
- **AIOS panel** — Proxmox Tower page in Void Mode.
- **Deploy App Vault → Proxmox LXC** — creates an LXC via Proxmox API (auto-VMID, nesting, DHCP, onboot), returns a copy-pasteable Docker bootstrap script.

### Added — AI integration (Phase 4)

- **Odysseus integration** — scoped API tokens, SSE event stream, webhook handler, manifest endpoint.
- **AI context endpoint** — `/api/ai/context` returns a structured codebase map for Odysseus system prompt injection.
- **MCP server** — Model Context Protocol server at `/api/mcp` with tools: `list_routes`, `read_file`, `search_code`, `get_template`, `list_containers`, `list_services`, `list_secrets`, `get_policy_rules`, `get_audit_log`, `get_alerts`, `get_tags`, `get_timeline`, `get_backups`.
- **Odysseus webhook** — handles `container.*` and `service.*` actions (start/stop/restart) dispatched from automations; policy-checked.
- **AI ask popup** — `/ask` chat overlay wired to local llama.cpp or Ollama.

### Added — App Vault

- **Catalog** — 40+ curated self-hosted apps with Docker Compose definitions, port mappings, AI integration badges.
- **Deploy** — one-click deploy with automatic conflict detection (system-installed services), GPU requirement stripping.
- **Embed proxy** — port-based nginx proxy (8800–8899 range) for in-UI app iframes; no DNS required; firewall opened automatically.
- **Custom deploy** — paste any Docker Hub image; configure ports, volumes, env vars; live YAML preview.
- **AI Discover** — natural language search for self-hosted apps via local LLM.
- **Lifecycle** — start, stop, restart, redeploy (compose up --build), remove with dry-run guard.
- **External stack adoption** — detect and manage Docker Compose stacks not deployed by VoidTower.
- **Deploy to Proxmox** — create an LXC on any configured Proxmox host and bootstrap the app inside it.

### Added — Plugin system

- **Plugin registry** — SQLite-backed; plugins stored in `{data_dir}/plugins/{id}/`.
- **Install from URL** — download a `.zip`, extract, read `plugin.json` manifest; handles top-level subdirectory prefix.
- **Static serving** — `/plugin-assets/:id/*` serves plugin files (auth-gated, no restrictive CSP).
- **Iframe host** — each plugin gets a full-page iframe at `/plugins/:id`; `sandbox="allow-scripts allow-same-origin allow-forms"`.
- **Dynamic sidebar** — enabled plugins appear under a "Plugins" section in the nav.
- **`plugin.json` manifest** — `id`, `name`, `version`, `description`, `author`, `entry`, `icon`, `nav_group`.

### Added — Infrastructure management

- **Services** — systemd service list, status, start/stop/restart, logs, policy enforcement for API tokens.
- **Containers** — Docker container list, actions, live log stream WebSocket, exec shell, compose view/edit/propose/apply, policy enforcement.
- **VMs** — local KVM management via libvirt.
- **Storage** — block devices, mounts, fstab editor, SMART data, RAID status/create/stop, format, configurable paths.
- **Network** — LAN neighbor scan.
- **WireGuard** — peer management (requires `wireguard` capability).
- **Firewall** — UFW rule management with dry-run guard (requires `ufw` capability).
- **Proxies** — nginx reverse proxy config CRUD; Docker nginx-proxy container management; embed proxy auto-creation.
- **Files** — file browser with read, write, mkdir, delete, rename, raw serve.
- **Terminal** — local PTY shell + SSH sessions; xterm.js frontend; multi-tab; clipboard; fish/bash/zsh support.
- **Backups** — restic backup configs; scheduled jobs; restore-test runner with confidence widget; daily alert for untested backups.
- **Updates** — VoidTower self-update (bare-metal + Docker), Odysseus update, OS package updates, Docker image updates.

### Added — Observability

- **Dashboard** — CPU, RAM, swap, disk, load average, GPU (NVIDIA/AMD/Vulkan), process count, OS info, uptime.
- **Metrics** — live WebSocket stream; broadcast channel for internal consumers.
- **Alerts** — threshold-based (CPU >85/95%, RAM >80/92%, disk >80/92%); status check alerts; VM state-change alerts; backup never-tested alert. Acknowledge, resolve, delete.
- **Timeline** — global audit-style event log across all resource types.
- **Status checks** — HTTP/TCP uptime checks on a configurable interval; public status page at `/status`.

### Added — Organisation & UX

- **Resource tags** — create, assign, filter across containers, services, apps, proxies, backups, VMs, Proxmox VMs. Global tag filter in TopBar.
- **Nav customisation** — drag-to-reorder nav groups and items; inline group rename; capability-gated items hidden when feature not detected.
- **Themes** — built-in theme library + custom theme builder; transparency and panel rounding sliders; import/export; live preview.
- **Void Mode (AIOS)** — spatial windowing OS shell with 21 native panels; Odysseus AI assistant pane; split-pane layout; device-tier detection.
- **Instance branding** — custom instance name (title bar + sidebar), custom CSS injection, favicon swap.
- **Automation** — cron-style scheduled jobs; manual run; run history; webhook trigger.
- **Integrations** — API token CRUD with scopes; Odysseus config; SSE event stream; notification webhooks (ntfy, Discord, Slack).
- **Models** — llama.cpp model download + load/unload; Ollama pull, create, delete.
- **Mods** — git-based code patch system for community extensions.
- **Capabilities** — auto-detect host features (Docker, systemd, KVM, WireGuard, UFW, etc.); capability page at `/capabilities`.
- **Settings** — general (instance name, logo, custom CSS), AI URL, notification, nav config, branding.

### Changed

- Error messages from Proxmox now surface to the frontend (previously showed generic "internal error").
- Installer bind address defaults to `0.0.0.0`; shows detected LAN IP in post-install summary.
- App embed proxies use port-based nginx configs — no DNS or server_name needed.

### Infrastructure

- Backend: Rust, axum 0.7, sqlx/SQLite, tokio
- Frontend: React 18, TypeScript, Vite, Zustand, Tailwind
- AI: Odysseus (Python/uvicorn) on :7000
- Terminal: portable-pty + xterm.js over WebSocket
- Auth: session cookie `vt_session`; bearer token auto-session injection
- CI: `cargo clippy --all-targets --all-features -- -D warnings` + `npm run build`

---

## [0.1.0] — initial

Initial commit. Proof-of-concept with Ollama model detection, basic dashboard, and early installer scaffold.
