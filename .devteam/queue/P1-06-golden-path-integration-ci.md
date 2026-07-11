# Task P1-06: Golden-path CI integration job (app deploy, restic backup/restore-test, container lifecycle)

## Status: Ready
**ADR:** ADR-005
Depends-On: none
Requires-Path: .github/workflows/ci.yml, backend/src/backups/mod.rs, backend/src/containers/mod.rs, backend/src/api/apps.rs, backend/src/api/backups.rs, backend/src/api/containers.rs

## Source

- `docs/gap-analysis.md` §3, P1 table row 5: "Golden paths | CI integration job
  (Docker-in-Docker): deploy one App Vault app end-to-end; restic backup → restore-test;
  container lifecycle | Proves the product's core promise per commit."
- `docs/edd.md` §13 (Testing Requirements): "Integration | App deploy end-to-end against real
  Docker; Proxmox adapter against recorded fixtures (wiremock) in CI + a
  `just test-pve-live` target run manually against the real homelab before release |
  testcontainers, wiremock | CI (mocked), manual (live)."
- `docs/adr/ADR-005-p1-ci-hardening.md` — **authoritative** for the grant and constraint 4:
  this job runs against real Docker-in-Docker in the CI runner; it does **not** attempt to
  reach a real or simulated Proxmox host — that's out of scope (manual, pre-release, per the
  EDD row above), not something to fake with wiremock in this task unless a later task's spec
  says otherwise.
- `docs/codebase-map.md` §1/§2 — `backend/src/backups/mod.rs` (restic wrapper: run/check/
  restore-test), `backend/src/containers/mod.rs` (bollard/Docker wrapper), `api/apps.rs`
  (App Vault deploy/lifecycle), `api/backups.rs`, `api/containers.rs` (the HTTP handlers
  around the above).

## Scope

A new CI job that, against real Docker-in-Docker in the runner:

1. **Deploys one App Vault app end-to-end** via the real deploy path (`POST /api/apps/deploy`
   or `deploy-custom`, per `api/apps.rs`) against a real `docker compose` invocation, asserts
   it reaches a running/healthy state, then tears it down.
2. **Restic backup → restore-test**, against a real (test-scoped, ephemeral) restic
   repository: create a `backup_configs` row, run it (`api/backups.rs`'s `run`), then run its
   `restore-test` action, asserting the confidence/status fields the existing restic wrapper
   (`backend/src/backups/mod.rs`) already reports.
3. **Container lifecycle**: start/stop/restart a real container via `api/containers.rs`'s
   `action` handler, asserting each transition against real `docker inspect` state, not a
   mock.

Pick a minimal, deterministic App Vault manifest for step 1 — check `docs/gap-analysis.md`'s
mention of "52 App Vault apps" and find (or, if none is suitable, note why in the PR
description and pick the closest fit) one with no external network dependency and a fast,
predictable healthcheck, so the job is reliable in CI rather than flaky. Do not invent a new
manifest under this ADR — this task has no grant to add product features or new apps; if the
existing catalog has nothing suitable, escalate.

## Contract (verbatim, `docs/gap-analysis.md` §3)

> Golden paths | CI integration job (Docker-in-Docker): deploy one App Vault app end-to-end;
> restic backup → restore-test; container lifecycle | Proves the product's core promise per
> commit

## Architecture decisions already made (ADR-005 — do not re-litigate)

- Docker-in-Docker only; no live or simulated Proxmox target in this job.
- New dev-dependencies (e.g. `testcontainers`) are permitted under ADR-005's Cargo.toml
  grant, dev-dependencies only, and must be justified in the PR body against what
  `tokio-test`/`tower`'s `util` feature (already present) don't already cover.
- The new job is additive; existing `frontend`/`backend` jobs untouched.

## Files to touch

- `.github/workflows/ci.yml` — **forbidden zone, ADR-005**, new job only. Needs Docker
  available in the runner (GitHub-hosted `ubuntu-latest` runners have Docker preinstalled;
  verify `docker compose` v2 is present or install it as a step) and `RESTIC_PASSWORD` set to
  a test-only value (never a real secret — `backend/src/api/backups.rs`'s
  `restic_password()` already defaults to `"changeme"` if unset, which is sufficient for an
  ephemeral CI repo; do not introduce a GitHub Actions secret for this).
- `backend/Cargo.toml`/`Cargo.lock` — **forbidden zone, ADR-005**, dev-dependencies only, if
  `testcontainers` (or similar) is needed.
- New: integration test file(s) under `backend/tests/` (e.g. `backend/tests/golden_path.rs`)
  — not a forbidden zone; this is where the actual end-to-end assertions live, run via
  `cargo test --test golden_path` from the new CI job.

## Explicitly not to touch

- Any Proxmox-related code path (`api/proxmox.rs`, `vms/mod.rs`) — out of scope per ADR-005
  constraint 4.
- `backend/src/backups/mod.rs`, `backend/src/containers/mod.rs`, `api/apps.rs`,
  `api/backups.rs`, `api/containers.rs` themselves — this task tests existing behavior
  through its public HTTP/CLI surface; it does not modify the product code under test unless
  the golden path reveals an actual bug, in which case: stop, do not fix product code under a
  CI-tooling ADR, escalate.
- Any existing job in `ci.yml`.

## Acceptance tests (name before implementing)

- `app_vault_deploy_reaches_healthy_state_end_to_end` — real `docker compose up` via the
  deploy handler, polling actual container health, not a mocked bollard client.
- `app_vault_teardown_removes_containers_and_optionally_volumes` — exercises both the
  destructive-but-not-irreversible stop/remove path and (separately) the
  `delete-volumes`/`purge` path already reclassified `Irreversible` by P0-04's denylist —
  confirm this test does **not** attempt to bypass that classification; it exercises it
  directly as a human/CI-actor action (session-authenticated test harness), which is outside
  the AI-ingress voidwatch gate by design (per `docs/codebase-map.md` §6's actor-detection
  note) — do not add any voidwatch-bypass logic to make this test pass.
- `restic_backup_then_restore_test_reports_confidence` — real restic repo, real backup run,
  real restore-test, asserting the `last_restore_test_status`/confidence fields
  `backend/src/backups/mod.rs` already computes.
- `container_lifecycle_start_stop_restart_reflect_real_docker_state` — each transition
  checked against actual `docker inspect` output (via bollard or the `docker` CLI directly in
  the test), not an in-memory fake.
- `golden_path_job_runs_on_pull_request_and_push_to_main` — CI-config-shaped acceptance,
  matching P1-05's equivalent check: the new job actually gates merges.

## Forbidden zones for this task

`.github/workflows/ci.yml`, `backend/Cargo.toml`, `backend/Cargo.lock` (all ADR-005,
dev-dependencies and new-job-only).

## Review tier

Boundary review (EDD §15.5 — integration/CI tooling over `vt-apps`/`vt-jobs`-equivalent
surface, not the security boundary itself), except the `.github/workflows/ci.yml` diff
specifically, which gets full line review per CLAUDE.md's CI forbidden-zone treatment.
