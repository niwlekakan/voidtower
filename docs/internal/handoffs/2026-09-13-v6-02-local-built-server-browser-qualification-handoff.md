# 2026-09-13 — V6-02 local built-server/browser qualification handoff

Status: runtime-verified
Tracked plan slice: V6-02 — web client qualification at a configured built-server boundary
Outcome: authenticated durable SSE first-frame delivery, durable event delivery, browser reconnect/replay, stale-cursor gap recovery, restart/reconnect, and dashboard accessibility verified through local Chromium.
Branch: dev
HEAD: f94248d0f018ea494def20946e6bcc51e637c47f
Upstream: dev ahead 1 / behind 0; no push performed

## Boundary and safety

- Qualification used the source-built image `vtqual-local-current` (`sha256:01f4827a49f4778da94d33b11fb4d20703a0d8073a156ef8556013475ddd2c4d`) built from the current checkout.
- Disposable Compose project: `vtqualfix`.
- Ports were explicitly overridden to loopback only: `127.0.0.1:80`, `127.0.0.1:443`, and `127.0.0.1:8745`.
- The qualification volumes and network were disposable and removed/recreated during the run. No persistent VoidTower volumes were used.
- The coding worker remained Docker-backed and was not granted Docker CLI or `/var/run/docker.sock`; host Docker was used only by the bounded qualification procedure.
- Chromium execution boundary: local `agent-browser` session `hermes-voidtower-v6-02-local` against `http://127.0.0.1/`. The generic remote Browser Use route rejected the private URL and was not used as authoritative evidence.
- Temporary bootstrap credentials, TOTP secret, session state, and passwords were held outside the repository and deleted after probing.

## Verification evidence

- `runtime-verified`: container reported `running/healthy`; `GET http://127.0.0.1/api/health` returned HTTP 200 and `{"status":"ok","version":"0.9.0"}`.
- `runtime-verified`: direct backend `GET /api/events/stream?after=0` returned HTTP 200, `text/event-stream`, and an initial `event: stream.ready` frame. The long-lived curl ended with timeout 28 after bytes were captured; this is expected for an open SSE connection.
- `runtime-verified`: loopback nginx `GET /api/events/stream?after=0` returned HTTP 200, `text/event-stream`, `X-Accel-Buffering: no`, and an initial `event: stream.ready` frame. The long-lived curl ended with timeout 28 after bytes were captured.
- `runtime-verified`: owner account creation and TOTP enrollment completed through the local Chromium UI; browser reached `/dashboard`.
- `runtime-verified`: browser `EventSource('/api/events/stream?after=0')` opened once with no initial error and received `stream.ready` with `cursor=0`, `high_water=0`.
- `runtime-verified`: supported `POST /api/cmdb/assets` returned HTTP 200 and produced durable event sequence 1 (`cmdb.asset.created.v1`); the authenticated browser stream received `durable_event` with `lastEventId=1`.
- `runtime-verified`: restarting the disposable `voidtower` container caused one browser error followed by a second open; the reconnect received `stream.ready` with `cursor=1`, `high_water=1`, proving replay cursor propagation across restart.
- `runtime-verified`: a second authenticated browser stream using `after=999` received `stream.gap` with `reason=future_cursor`, `requested_after=999`, `earliest_available=1`, and `latest_available=1`.
- `runtime-verified`: accessibility snapshot contained the `Dashboard` heading, `Dashboard` navigation link, `Logout` button, and `Settings` link. Snapshot and screenshot are preserved at:
  - `docs/internal/evidence/v6-02/2026-09-13-local-chromium-accessibility-snapshot.txt`
  - `docs/internal/evidence/v6-02/2026-09-13-local-chromium-dashboard.png`
- Evidence SHA-256:
  - snapshot: `9bf9dd398667a90d838a2f1ab46832cf7140613d5c6a0391551e21ddc8bc62c4`
  - screenshot: `723befc30ddc354ac4262339f0b7f1d00a75074ca4d0412e3a16f06b30b78533`

## Source and repository state

- No production source behavior changed during this qualification.
- The current two staged paths remain preserved and excluded from this handoff and any commit:
  - `backend/src/agent/mod.rs`
  - `backend/src/agent/state.rs`
- The evidence files above are the only new qualification artifacts.
- This runtime evidence resolves the prior blocker: the earlier missing first frame was caused by testing a stale source-built image whose generated nginx configuration lacked the current dedicated SSE locations. Rebuilding from the current checkout produced the expected direct and proxy frames.
- This does not establish `release-qualified`; install/upgrade/recovery and the remaining tracked 1.0 gates are outside this slice.

## Reusable next action

1. Review this handoff and the two evidence artifacts independently.
2. Run the normal project verification and review gates without staging the two unrelated agent files.
3. If review passes, commit only this handoff and the two evidence artifacts, then let the supervisor resume the next approved slice. Do not push `main` or publish unrelated staged work.
