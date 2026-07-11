# Task P1-05: cargo-deny/cargo-audit CI job + CODEOWNERS forbidden-zone protection

## Status: Ready
**ADR:** ADR-005
Depends-On: none
Requires-Path: .github/workflows/ci.yml

## Source

- `docs/gap-analysis.md` §3: "Also in P1: adopt the EDD §12 CI additions
  (cargo-deny/audit, forbidden-zone path protection via CODEOWNERS — which now protects
  `policy.rs`/`voidwatch`, auth, secrets, `db/mod.rs`, CI itself)..."
- `docs/edd.md` §12 (CI/CD Strategy): `ci.yml` should run `cargo deny check`; §14 (Coding
  Standards) references dependency-graph discipline. `nightly.yml` (not present in this
  repo and not this task's scope) is where the EDD puts `cargo audit` for a greenfield
  workspace — this task adds both `cargo-deny` and `cargo-audit` to the existing `ci.yml`
  instead, since this repo has no `nightly.yml` and gap-analysis explicitly pairs them in one
  P1 line item.
- `docs/adr/ADR-005-p1-ci-hardening.md` — **authoritative** for the grant; covers this task
  alongside P1-03 and P1-06 (all three add a job to `ci.yml`).
- CLAUDE.md's own "Forbidden-zone grants (ADRs)" section and the operator guide's one-time
  setup step 2 ("Protect forbidden zones: `.github/CODEOWNERS` mapping the paths above to
  you") — this task delivers the file the operator guide assumed already existed; it does
  not.

## Scope

Two independent, additive pieces:

1. **`cargo-deny` + `cargo-audit` CI job.** New job in `.github/workflows/ci.yml`: run
   `cargo deny check` (needs `backend/deny.toml`, which does not exist yet — create it with
   a reasonable baseline: deny unmaintained/yanked crates and known-vulnerable advisories,
   allow the license set this crate already uses — check `backend/Cargo.toml`'s dependency
   list and the crate's own `license = "AGPL-3.0-or-later"` for what to allow) and
   `cargo audit` (via `rustsec/audit-check` action or `cargo install cargo-audit` — pick one,
   document why). Failing either blocks merge, same as the existing `backend` job.
2. **`.github/CODEOWNERS`.** New file mapping CLAUDE.md's forbidden-zone paths to the
   operator's GitHub identity (check `git log`/existing PR merge commits for the correct
   handle — `docs/adr/ADR-003-auth-scope-enforcement.md`'s signature line and this repo's
   remote both reference `niwlekakan`; verify rather than assume). Per ADR-005's Decision
   section, map: `backend/src/policy.rs`, `backend/src/voidwatch/**`, `backend/src/auth/**`,
   `backend/src/api/auth.rs`, `backend/src/oidc.rs`, `backend/src/api/secrets.rs`,
   `backend/src/db/mod.rs`, `.github/workflows/**`, `scripts/devteam/**`, `CLAUDE.md`,
   `docs/edd.md`, `docs/gap-analysis.md`, `docs/adr/**`. This file alone does not enforce
   anything — GitHub only requires code-owner review if branch protection is configured to
   require it, which is a repo-settings change the operator makes by hand after this PR
   merges (note this explicitly in the PR description so it isn't mistaken for done).

## Contract (verbatim, `docs/gap-analysis.md` §3)

> Also in P1: adopt the EDD §12 CI additions (cargo-deny/audit, forbidden-zone path
> protection via CODEOWNERS — which now protects `policy.rs`/`voidwatch`, auth, secrets,
> `db/mod.rs`, CI itself)...

## Architecture decision already made (ADR-005 — do not re-litigate)

- New jobs are additive; the existing `frontend`/`backend` jobs are untouched.
- CODEOWNERS mirrors CLAUDE.md's forbidden-zone list exactly; if they ever drift, CLAUDE.md
  wins and CODEOWNERS should be corrected to match.
- No branch-protection configuration is part of this task — that's a GitHub repo setting,
  not a file in this tree, and stays with the operator.

## Files to touch

- `.github/workflows/ci.yml` — **forbidden zone, ADR-005**, new job(s) only.
- New: `.github/CODEOWNERS` — **forbidden zone, ADR-005**.
- New: `backend/deny.toml` — **forbidden zone, ADR-005**.

## Explicitly not to touch

- The existing `frontend`/`backend` jobs in `ci.yml`, or any other workflow file
  (`docker.yml`, `release.yml`, `desktop-build.yml`).
- `scripts/devteam/**` — the harness itself, untouched regardless of ADR.
- Any GitHub repo settings (branch protection rules) — not achievable from within this
  sandbox anyway; note what the operator still needs to do by hand in the PR description.

## Acceptance tests (name before implementing)

Not a Rust-test-shaped task — acceptance here is CI-job-shaped. Name and satisfy:

- `deny_toml_baseline_passes_against_current_dependency_tree` — running `cargo deny check`
  locally against `backend/deny.toml` and the current `Cargo.lock` must pass clean before
  this PR ships; a baseline config that immediately fails CI on unrelated pre-existing
  findings is not acceptable — either the config's severity thresholds are set to catch new
  problems going forward (not retroactively fail on everything already present), or genuine
  pre-existing findings are fixed as part of this task if trivial (a version bump), or
  escalated if not.
- `cargo_audit_baseline_passes_against_current_dependency_tree` — same requirement for
  `cargo audit`.
- `codeowners_paths_match_claude_md_forbidden_zones_exactly` — a reviewable check (can be
  manual, documented in the PR body, or a small script) that every forbidden-zone path listed
  in CLAUDE.md's "Forbidden zones" section has a corresponding CODEOWNERS entry, and vice
  versa (no CODEOWNERS entry for a path CLAUDE.md doesn't list).
- `new_ci_jobs_run_on_pull_request_and_push_to_main` — matches the existing jobs' `on:`
  triggers, so the new jobs actually gate merges rather than running only on a schedule.

## Forbidden zones for this task

`.github/workflows/ci.yml`, `.github/CODEOWNERS`, `backend/deny.toml` (all ADR-005).

## Review tier

Full line review (EDD §15.5 — CI workflow and forbidden-zone protection are explicitly
full-line-review tier per CLAUDE.md's forbidden zones list).
