# ADR-005 — CI hardening grant for Phase P1 (verification retrofit)

**Status:** Accepted (signed by operator niwlekakan on 2026-07-11)
**Date:** 2026-07-11
**Expires:** at P1 phase exit (gap-analysis §3 exit criteria met and verified by the operator)

## Context

`docs/gap-analysis.md` Phase P1 requires additive changes to `.github/workflows/ci.yml` —
a mutation-testing job for the policy engine, a `cargo-deny`/`cargo-audit` job, and a
Docker-in-Docker golden-path integration job — plus a new `.github/CODEOWNERS` file mapping
CLAUDE.md's forbidden zones to the operator, per gap-analysis §3 ("adopt the EDD §12 CI
additions ... and forbidden-zone path protection via CODEOWNERS"). `.github/workflows/` is a
hard forbidden zone (CLAUDE.md, `scripts/devteam/gates.sh`'s `BLOCKLIST`); without a grant,
every P1 task that needs a new CI job correctly refuses and escalates.

Three P1 tasks independently need to add a *job* (not modify an existing one) to
`ci.yml`: **P1-03** (cargo-mutants targeting `policy.rs`/`voidwatch/**`), **P1-05**
(cargo-deny/cargo-audit + CODEOWNERS), and **P1-06** (golden-path Docker-in-Docker
integration). Rather than one ADR per task, this grant covers all three — see CLAUDE.md's
own instruction to group related tasks under one ADR.

## Decision

Task specs numbered `P1-*` that cite **ADR-005** are granted modification rights to exactly
the paths below:

```granted-paths
.github/workflows/ci.yml
.github/CODEOWNERS
backend/Cargo.toml
backend/Cargo.lock
backend/deny.toml
```

In prose:

- `.github/workflows/ci.yml` — **new jobs only**, appended to the existing `jobs:` map.
  The existing `frontend` and `backend` jobs (fmt-check via `gates.sh`'s own G0, clippy,
  `cargo test --all-features`) may not be removed, renamed, weakened, or have their
  `-D warnings` / `--all-features` flags dropped.
- `.github/CODEOWNERS` — new file. Maps the forbidden-zone paths listed in CLAUDE.md
  (`backend/src/policy.rs`, `backend/src/voidwatch/**`, `backend/src/auth/**`,
  `backend/src/api/auth.rs`, `backend/src/oidc.rs`, `backend/src/api/secrets.rs`,
  `backend/src/db/mod.rs`, `.github/workflows/**`, `scripts/devteam/**`, `CLAUDE.md`,
  `docs/edd.md`, `docs/gap-analysis.md`, `docs/adr/**`) to the operator's GitHub identity.
  This file does not by itself enforce anything (branch protection requiring code-owner
  review is a GitHub repo setting, not committed config) — it is the declarative map the
  operator turns into a required-review rule by hand, same division of labor as ADR signing.
- `backend/Cargo.toml` / `Cargo.lock` — **dev-dependencies only**, and only the specific
  crates named in each citing task's spec (e.g. `tower` `util` feature is already present;
  a golden-path task may need `testcontainers` and/or `wiremock`). No `[dependencies]`
  (non-dev) additions under this grant — a runtime dependency is a heavier call CLAUDE.md
  reserves for its own ADR judgment, not a blanket pre-authorization.
- `backend/deny.toml` — new file, `cargo-deny` configuration (license/advisory/ban lists).

## Explicitly NOT granted

- Any existing job's steps, triggers, or `runs-on`/permissions block in `ci.yml` — additive
  only. If a task believes an existing job must change, it escalates.
- `.github/workflows/docker.yml`, `.github/workflows/release.yml`,
  `.github/workflows/desktop-build.yml` — out of scope for P1.
- `scripts/devteam/**` (the harness itself) — untouched by this ADR, same as CLAUDE.md's
  standing rule.
- `[dependencies]` (non-dev, shipped in the release binary) in `backend/Cargo.toml`.
- Branch-protection / required-status-check configuration in the GitHub repo settings
  (not a file in this tree) — the operator applies this by hand after CODEOWNERS lands.
- Everything else in CLAUDE.md's forbidden list.

## Constraints

1. **Additive only.** A new job is a new top-level key under `jobs:`. Do not restructure
   `ci.yml`'s existing jobs to "share" steps with a new one in ways that touch their bodies.
2. Every new job must be **independently useful on a failing/missing-tool run** — e.g. the
   cargo-mutants job should fail loudly (not silently skip) if `cargo-mutants` isn't
   installable in the runner, not swallow the error.
3. The mutation-testing job (P1-03) targets `backend/src/policy.rs` and
   `backend/src/voidwatch/**` only — not the whole crate; mutation testing whole-crate is
   prohibitively slow and gap-analysis scopes it to the security boundary specifically
   (EDD §13's "90% line coverage on `vt-voidwatch`" row, retargeted to this repo's shape).
4. The golden-path job (P1-06) runs against real Docker-in-Docker in the runner (EDD §12/§13:
   testcontainers) — it does **not** attempt to reach a real Proxmox host or the nested-PVE
   fixture environment; that remains a manual `just test-pve-live`-style target per EDD §13,
   out of scope for unattended CI. If a citing task's contract needs Proxmox coverage, it
   uses recorded fixtures (wiremock) against `api/proxmox.rs`'s HTTP client, not a live host.
5. CODEOWNERS entries mirror CLAUDE.md's forbidden-zone list exactly — if the two drift,
   CLAUDE.md is authoritative and CODEOWNERS should be corrected to match, not the reverse.
6. New dev-dependencies are justified in the PR body: what it's for, why an existing
   dev-dependency (`tokio-test`, `tower/util`) doesn't already cover it.

## Consequences

P1-03, P1-05, and P1-06 unblock without opening the rest of `.github/workflows/` or the
harness. Because all three land as separate PRs reviewed serially by the operator, this
grant does not need to sequence them against each other — each branches from the `main` the
prior one merged into, same as any other pair of tasks sharing a file. At P1 exit this ADR
is revoked and `.github/workflows/` / `.github/CODEOWNERS` close again (CODEOWNERS itself,
once merged, is expected to persist as committed config — "closing the zone" means future
*changes* to it need a fresh grant, not that the file is removed).
