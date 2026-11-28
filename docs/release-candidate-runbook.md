# Release-candidate qualification runbook

This runbook is the operator sequence for R0-02. It keeps source/unit evidence separate from integration, runtime, and release qualification. A command that passes locally does not authorize publication unless the exact candidate, required external evidence, archive matrix, lifecycle checks, and independent review evidence also pass.

## 1. Establish candidate ownership

Run from the repository root. Do not stash, reset, clean, or overwrite paths owned by another workstream.

```sh
git status --short
git branch --show-current
git rev-parse HEAD
PYTHONDONTWRITEBYTECODE=1 python3 scripts/repo_truth.py --repo . --json --check
bash scripts/check-repository-hygiene.sh
bash scripts/check-schema-migration-ownership.sh
git diff --check
```

The source-truth report is inventory evidence only. `runtime_support_claimed: false` is expected until named runtime checks have actually passed. The release candidate must have no staged or modified paths when `--require-clean` is used. Resolve ownership with the responsible workstream; do not make unrelated formatting, build-output, or generated-bytecode changes just to obtain a clean report.

## 2. Run local contract checks

These checks exercise the source-owned release runner, publication validator, manifest, and governance prerequisites without claiming host runtime or release support.

```sh
PYTHONDONTWRITEBYTECODE=1 python3 -m unittest \
  scripts.test_release_gate \
  scripts.test_release_evidence \
  scripts.test_repository_prerequisites \
  scripts.test_action_schema_catalog \
  scripts.test_mcp_dependencies \
  scripts.test_compose_contract -q
PYTHONDONTWRITEBYTECODE=1 python3 -m py_compile \
  scripts/release_gate.py \
  scripts/validate_release_evidence.py \
  scripts/test_release_gate.py \
  scripts/test_release_evidence.py
python3 -m json.tool scripts/release-gates.json >/dev/null
```

The focused suite must pass on the final candidate. Do not substitute a fixture-only dependency environment for the isolated environment used by the release runner; Cargo gates consume only the trusted user-local `registry`/`git` caches through read-only descriptor-bound mounts and run with `CARGO_NET_OFFLINE=true`, so the candidate host must prewarm those caches before the isolated command. The report must identify the actual command and environment that produced it.

## 3. Execute the clean-candidate manifest

Use a repository-contained, unique output path. The runner refuses unsafe or pre-existing output paths and does not modify the checkout.

```sh
REPORT="dev-data/release-gate-$(git rev-parse HEAD).json"
PYTHONDONTWRITEBYTECODE=1 python3 scripts/release_gate.py \
  --repo . \
  --manifest scripts/release-gates.json \
  --scope all \
  --require-clean \
  --json \
  --output "$REPORT"
```

Exit `0` means every selected required executable gate passed and no required gate was skipped. Exit `1` is a completed diagnostic with a failed, blocked, skipped, interrupted, or dirty-candidate result. Exit `2` means the invocation or safety boundary was rejected. Preserve a report only when it is evidence for the exact candidate; remove temporary diagnostic reports after inspection.

The report must show the exact full commit, empty `git.staged_paths` and `git.modified_paths`, zero skipped mandatory gates, selected subsystems matching the manifest, bounded redacted diagnostics, and hashes for any artifacts declared by the executable manifest. The current source-owned manifest declares no executable-gate artifacts; the required release archive hashes are collected and validated in the external/package and publication steps below. `evidence_limit` is a ceiling and never promotes unit or integration evidence to runtime or release qualification. If `--require-clean` rejects a dirty checkout and an output path was supplied, the persisted report is an explicit failed diagnostic with every selected gate blocked; it cannot be used by the publication validator. Report bytes are flushed before the runner returns and the report directory is flushed after creation, so the retained report is the durable diagnostic boundary rather than merely stdout output. If shutdown requires rewriting an already-created report, the runner flushes the replacement bytes and then the retained parent directory before closing the file; it revalidates the no-follow path after writing and after the directory flush. If rewrite or directory flush fails, Linux cleanup atomically moves the report into a unique mode-0700 quarantine directory with `renameat2(RENAME_NOREPLACE)`, validates the moved entry through a retained no-follow descriptor, rechecks device/inode/mode before moving the entry into a private tombstone, and restores replacement paths via descriptor-bound no-replace links without overwriting them. Initial report-creation failures use descriptor-only invalidation when report identity capture is unavailable: the retained report inode is truncated and fsynced while the public pathname is never moved, preserving any concurrent replacement. Other initial report-creation failures and late shutdown rewrites use the same private-quarantine recovery contract; unsupported identity-bound cleanup fails closed. Every captured evidence ancestor is revalidated component-by-component without following symlinks before the report path is accepted; if interrupted-report recovery itself fails, the runner returns exit `2` without emitting the report, rather than allowing a passed report to be claimed.

The runner bounds the complete serialized report at 1 MiB, writes new reports as invoking-user-owned `0600` regular files, and includes that mode in recovery identity. When `dev-data/` is created, the repository-root directory descriptor is flushed before the report is opened. Existing evidence ancestors are bound to captured device/inode identities and must be invoking-user-owned, non-group/other-writable directories before a report is created or accepted at the public path; descriptor-bound recovery invalidation still removes a report when an ancestor becomes unsafe. Quarantine directories are checked for exact `0700` mode, invoking-user ownership, and retained device/inode identity before use; the matching report inode is opened with nonblocking no-follow flags, validated as a regular file, and truncated through its identity-checked descriptor, leaving at most eight bounded empty tombstones per output filename. The runner never reopens or removes a quarantine pathname after the check, because preserving an empty private quarantine is safer than risking removal of a concurrent replacement. A denied or otherwise unexpected `renameat2` capability probe, or any cleanup failure, is a safety rejection rather than a publishable diagnostic.

## 4. Collect required external and packaged evidence

The source-owned manifest declares external gates that the local runner cannot synthesize:

- `golden-path-runtime`: run the documented Docker/restic golden path against the exact x86_64 archive produced by the build matrix. Record only the bounded marker metadata and SHA-256, not credentials or marker contents.
- `secret-history-scan`: run the repository-host secret-history check at the exact candidate commit and retain the declared evidence filename and digest.

The packaged runtime harness must use the exact archive selected by the build matrix. It must verify archive member safety, extract only the regular `voidtower` member, start that packaged binary, and exercise the named health/normal-operation path. Source-built or dirty-checkout health probes are not substitutes for packaged evidence.

The claimed candidate must also have the applicable installation, startup, normal-operation, backup/restore, upgrade, recovery, rollback, browser, and platform checks required by the tracked release plan. Record exact commands, versions, candidate/archive digests, and bounded results. A missing host service, browser, artifact, or external provider is a blocker, not a waived gate.

## 5. Validate the publication evidence set

After the build matrix and external jobs complete, place exactly the declared reports and the two version-matched architecture archives under the publisher's evidence root. Validate before generating checksums or invoking any release action:

```sh
python3 scripts/validate_release_evidence.py \
  --root artifacts \
  --manifest scripts/release-gates.json \
  --expected-reports 2 \
  --expected-commit "$GITHUB_SHA" \
  --publication-root published-artifacts \
  --checksums-output SHA256SUMS
```

The validator must re-check manifest/report command and evidence classifications, report provenance, external evidence, archive names and digests, safe paths, bounded closed schemas, and retained descriptor identity. It must fail before checksums/publication on missing, extra, mismatched, mutable, symlinked, or unsafe evidence. The resulting publication directory and checksum file are release artifacts; do not edit them after validation.

## 6. Independent review and promotion boundary

Before any protected publication, create a provisional local candidate containing only the release slice. Compute the exact review range and binary diff digest:

```sh
REVIEW_BASE="$(git rev-parse <review-base>)"
CANDIDATE_HEAD="$(git rev-parse HEAD)"
git diff --binary --no-ext-diff --no-renames "$REVIEW_BASE...$CANDIDATE_HEAD" \
  | sha256sum
```

Dispatch two fresh read-only reviewers against that exact base, head, and digest: one standards/spec reviewer and one security/release-boundary reviewer. Both must explicitly pass with no findings. The final continuity handoff must preserve the exact 40-character base/head values and 64-character lowercase digest. If either reviewer cannot run or fails, status is `review_blocked`; leave the candidate unpublished.

Protected publication is a separate operator-authorized action. This runbook does not push, create a release, bypass reviewers, or convert source/unit evidence into a release-qualified claim.
