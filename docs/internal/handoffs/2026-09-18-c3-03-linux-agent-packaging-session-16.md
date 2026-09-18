# 2026-09-18 — C3-03 Linux agent packaging session 16

Status: implemented and unit-verified; runtime/release-qualified remains blocked at the supported-host boundary
Tracked slice: C3-03 Linux agent release archive, installer lifecycle, and package contract
Milestone commit: 3f9f71353e515d0ac11200f3508ec401f02dfc41 (`feat(agent): harden Linux release packaging lifecycle`)
Branch: dev
Session timestamp: 2026-09-18T14:36:16Z

Implemented

- `scripts/build-release.sh` and `.github/workflows/release.yml` package the backend binary, frontend assets, and both controller/agent systemd units. Release tags normalize a leading `v`; published release architectures are x86_64 and aarch64.
- `scripts/install.sh` installs packaged runtime and systemd assets, renders explicit service paths, and stops/restarts controller and agent safely for install, update, repair, uninstall, and reset. `--skip-systemd` is honored across all maintenance lifecycle paths.
- Release archives are checked against basename-compatible `SHA256SUMS` entries with duplicate/malformed/case-normalized handling. Release, source, and catalog tarballs reject traversal and non-regular members and extract without archive ownership/permission preservation. Catalog extraction errors fail closed.
- Offline mode skips package-manager, source, catalog, model, and MCP pre-cache network operations; source builds use cache-only Cargo/npm behavior and explicit offline versions require an exact local `v<version>` tag.
- `scripts/test_agent_package.py` adds the focused archive/installer/systemd contract suite, including executable traversal and symlink-member rejection. `scripts/release-gates.json` declares the package gate and the release workflow invokes the actual manifest runner.
- `docs/agent/linux-agent-service.md`, `docs/internal/agent-knowledge/system-map.md`, and `docs/internal/agent-knowledge/documentation-backlog.md` record verified contracts, evidence, limitations, and follow-up documentation.

Independent review

- Final independent staged-diff review passed with no blocking security concerns or logic errors. Earlier review findings were fixed and re-reviewed: unsafe source/catalog extraction, checksum collisions, false-positive systemd detection, version drift, leading-v mismatch, unsupported architecture advertisement, offline npx/network fallback, reset ordering, skip-systemd lifecycle inconsistency, and ignored catalog extraction failures.

Verification evidence

- `PYTHONDONTWRITEBYTECODE=1 python3 -m unittest scripts.test_agent_package scripts.test_release_gate -v` — passed, 19 tests.
- `python3 scripts/test_release_gate.py` — passed, 11 tests.
- `bash -n scripts/install.sh && bash -n scripts/build-release.sh && python3 -m json.tool scripts/release-gates.json >/dev/null` — passed.
- `git diff --check` — passed for the final staged diff.
- `GITHUB_WORKSPACE="$PWD/.." cargo test --all-targets --all-features` from `backend/` — passed, 627 unit tests, 2 workflow-contract integration tests, and the example target.
- `python3 scripts/repo_truth.py --repo . --json --check` — passed; source inventory only and `runtime_support_claimed: false`.
- Final review was against the staged diff after all remediation; no external state was changed and no credentials were read or persisted.

Blocked or not qualified

- `python3 scripts/release_gate.py --repo . --manifest scripts/release-gates.json --scope changed --json` — blocked/failed in this sandbox because repository-truth subprocesses hit the constrained process boundary, hygiene rejects pre-existing tracked internal continuity paths, cargo-clippy and cargo-deny are unavailable, and the gate's backend test was terminated by the constrained environment. The package contract gate inside that report passed; this is not a release qualification result.
- `scripts/check-repository-hygiene.sh` — failed on pre-existing tracked internal handoff/evidence paths and knowledge paths; no cleanup was performed because those are continuity artifacts.
- `scripts/check-schema-migration-ownership.sh` — exited 0 but emitted `rg: command not found`; complete ownership verification is not established.
- `cargo fmt --check` — blocked because cargo-fmt is unavailable for toolchain 1.98.1.
- `cargo clippy --all-targets --all-features -- -D warnings` — blocked because cargo-clippy is unavailable for toolchain 1.98.1.
- Live systemd/systemd-analyze/device-host installation, service start/status, outbound collection/upload, upgrade/rollback, real release download/checksum, and published artifact qualification remain blocked. The Docker sandbox has no systemd manager/private socket, host `/dev` boundary, or Docker supervisor.

Preserved unrelated worktree state

- Modified but not included: `backend/src/api/apps.rs`, `scripts/release_gate.py`, and `scripts/test_release_gate.py`.
- Untracked and not included: `testing/` (existing frontend concept files).
- No unrelated paths were reset, staged, committed, or pushed.

Retrospective

- Learned: package security must validate source and catalog tarballs as well as release archives; release checksum manifests must use archive basenames after matrix artifact merging; lifecycle flags must gate controller and agent together; and release-tag normalization must happen at the installer argument boundary.
- Verified: archive layout, checksum enforcement, unsafe-member rejection, service-unit assets, systemd lifecycle contracts, offline network guards, focused package/release-gate tests, full Rust tests, source inventory, and final independent review.
- Remaining blocked: named supported Linux host/VM with real systemd and host `/dev`; active rustfmt/Clippy/cargo-deny toolchain; `rg`; trustworthy full release-gate execution; and published artifact/runtime qualification.
- Reusable commands: package suite above; `GITHUB_WORKSPACE="$PWD/.." cargo test --all-targets --all-features` from `backend/`; clean only generated `/tmp/vt-p1-*`, `/tmp/voidtower-*`, and `scripts/__pycache__/` after constrained test runs. Never remove unrelated worktree paths.
- End-user documentation changed: `docs/agent/linux-agent-service.md` now documents package contents, checksum/path validation, offline/version/architecture behavior, reset ordering, and the supported-host qualification boundary.

Next dependency-ready slice

Run the documented supported-host qualification on a named Linux VM/host with real systemd and host `/dev`, then capture install, enrollment, agent start, controller outage/recovery, update, rollback, and checksum artifact evidence. Do not promote runtime or release qualification from this sandbox.
