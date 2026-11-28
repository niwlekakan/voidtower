# Workflow governance

The `voidtower-dev` automation is intentionally split into four evidence tiers:

1. **Coding sandbox** — deterministic source, unit, contract, lint, type, and build checks. It has no host Docker socket, systemd control socket, credentials, or publication path.
2. **Runtime qualification** — disposable Docker/Compose, restic, systemd, browser, installation, upgrade, recovery, and rollback checks on a named host or isolated qualification VM. A sandbox pass never upgrades this tier.
3. **Independent review** — a provisional local candidate is reviewed by two fresh read-only reviewers: standards/spec and security/release boundary. The review is bound to the exact base SHA, candidate SHA, and SHA-256 of the reviewed binary diff. Self-review and old handoffs do not count.
4. **Promotion/publication** — release evidence, artifact hashes, provenance attestations, and an operator-controlled publication path.

The repository can validate that the workflow references `release`, but GitHub
environment protection is external state. Before enabling publication, verify
that the `release` environment exists with required reviewers, no self-review,
protected version-tag restrictions, and no bypass. The repository must retain
that verification as release evidence; YAML alone is not approval.

## Supervisor state contract

The supervisor must distinguish at least: `idle`, `running`, `awaiting_session_record`, `awaiting_session_terminal`, `failed_backoff`, `review_blocked`, `publish_backoff`, `publish_blocked`, `paused_usage`, `paused_usage_unknown`, `environment_blocked_continue`, `stale`, and `completed`.

A `review_blocked` state is not a quota or infrastructure retry. It requires a new independent review or operator intervention. It must never be cleared by self-review, a passing unit suite, or a prior handoff.

## Review reserve

With `oneshot_max_children: 6` and `max_concurrent_children: 4`, the head worker may use at most one read-only preflight specialist. Two delegation slots are reserved for the final reviewers. Implementation and fix workers may not consume the reserve. If the two reviewers cannot run, the candidate remains unpublished and the handoff is blocked.

## Evidence binding

A valid completion handoff must include:

- exact full review base SHA;
- exact full reviewed candidate SHA;
- SHA-256 of `git diff --binary --no-ext-diff --no-renames <base>...<candidate>`;
- explicit passing standards/spec review;
- explicit passing security/release-boundary review;
- explicit fresh independent reviewer context;
- verification results, limitations, and one next dependency-ready slice.

The final handoff/continuity commit may be newer than the reviewed candidate only when it adds continuity evidence and the reviewed candidate is contained in the final history.

## Docker image workflow

`.github/workflows/docker.yml` is build-only validation. It must not log in to
GHCR or push images merely because `main` or a version tag moved. OCI
publication belongs in a separate protected promotion workflow that downloads
an attested immutable image digest, verifies its provenance and candidate
identity, and exposes `packages: write` only after operator approval.

- GitHub Actions references are pinned to immutable commit SHAs.
- Pull-request CI uses concurrency cancellation; tagged releases do not cancel in flight.
- The `release` environment reference in the workflow is only an association; GitHub protection rules are configured outside the repository. Before enabling release promotion, configure required reviewers, prevent self-review, restrict allowed deployment refs to protected version tags, and disallow bypass. Verify those settings through repository administration/API and record the verification in the release evidence. YAML alone must never be treated as proof of approval.
- Dependabot updates Rust, npm, Python MCP, and GitHub Actions dependencies.
- Release artifacts remain hash-validated and are not published until all required build, runtime, secret-history, evidence, and approval gates pass.
- `packages: write` is confined to a future protected OCI promotion job; build-only Docker validation has no registry login or publication permission.
