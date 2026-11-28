# Release-candidate gate runner

VoidTower's release-candidate evidence collector is `scripts/release_gate.py`. It executes the repository-owned manifest at `scripts/release-gates.json` and writes a machine-readable report. The runner itself does not publish artifacts, stage or commit changes, or read environment files; declared gates may create their normal build and test outputs.

For a reproducible operator sequence that separates local contract checks from clean-candidate, external, artifact, runtime, and publication evidence, see [`docs/release-candidate-runbook.md`](release-candidate-runbook.md).

## Run the gates

From the repository root:

```sh
python3 scripts/release_gate.py --repo . --scope changed --json
```

Use `--scope all` for a release-candidate run that executes every declared gate:

```sh
python3 scripts/release_gate.py \
  --repo . \
  --scope all \
  --require-clean \
  --json \
  --output dev-data/release-gate.json
```

`--scope changed` selects governance gates plus gates whose declared subsystem appears in the source-truth changed-subsystem inventory. Required gates that are skipped because their subsystem is unchanged are recorded in `skipped_gates` and make the report fail closed. Use `--scope all` when the evidence must cover the complete candidate.
`--require-clean` performs the candidate cleanliness check immediately after
the source-truth report and before executing any gate. Without `--output`, it
returns exit `1` when staged or modified paths are present, so an operator or
CI job does not spend time running gates that cannot produce publishable
evidence. When `--output` is supplied for a dirty candidate, the runner still
does not start any gate, but persists a failed machine-readable report with
each selected gate marked `blocked` and the cleanliness error; this diagnostic
is never publishable because it records the dirty checkout. The flag does not
modify, stash, reset, or clean the checkout.

If an interrupt arrives while the initial source-truth preflight is still
running, the runner reuses that completed pre-execution snapshot instead of
starting a second inventory pass. It records the not-yet-run required gates as
blocked/skipped, persists the failed report, and exits non-zero; once a gate has
started, the runner performs the post-gate truth check before finalizing.

The `standalone-mcp-dependency-contracts` gate verifies that
`odysseus-mcp-servers/requirements.txt` contains the complete reviewed Linux
dependency closure with exact versions and that the release workflow installs
that source-owned file. The `standalone-mcp-tests` gate then requires those
dependencies in the active Python environment. Install the pinned file before
a local `--scope all` run under Python 3.13; the GitHub release build pins
Python 3.13 and installs the same file explicitly. The runner intentionally does not inherit an ambient `PYTHONPATH`
or package-configuration variables, so a dependency directory supplied only
through `PYTHONPATH` can support a direct fixture run but cannot satisfy the
release runner's isolated gate. Cargo gates use the trusted user-local Cargo
home only for its existing `registry` and `git` caches: those directories are
opened through identity-checked descriptors and mounted read-only into a
private `CARGO_HOME`; every gate receives that private home plus
`CARGO_NET_OFFLINE=true`, even when no trusted cache source is available, so
an incomplete cache fails promptly instead of waiting on the runner's unshared
network.

The output report contains the manifest schema version, Git state, platform information, selected subsystems, each gate's redacted explicit argument vector, bounded diagnostics, closed `evidence_limit` classification, skipped gates, and SHA-256 hashes for declared artifacts. `evidence_limit` is a ceiling, not a promotion: a `unit-verified` or `integration-verified` gate cannot by itself establish runtime or release qualification. The publication validator checks that every report gate carries the exact classification declared by the source-owned manifest. A persisted report must be a new path under the repository's `dev-data/` directory; the runner refuses to overwrite an existing file or follow an output symlink. Python gates run with `PYTHONDONTWRITEBYTECODE=1` so gate imports cannot create untracked bytecode in the candidate checkout and perturb the source-truth inventory. The runner flushes report bytes before returning and flushes the `dev-data/` parent after creating a report; an interrupted-report rewrite uses the same file flush and then flushes its retained parent directory before closing, with no-follow path revalidation after writing and after the directory flush. If rewrite or directory flush fails, Linux cleanup atomically moves the current directory entry into a unique `renameat2(RENAME_NOREPLACE)` quarantine directory, validates the moved entry through its retained no-follow descriptor, rechecks device/inode/mode before moving the entry into a private tombstone, and restores replacement paths through descriptor-bound no-replace links without overwriting them. When initial report identity capture is unavailable, cleanup truncates only the retained report descriptor and never mutates the public pathname, so a concurrent replacement remains untouched. The initial creation path applies the same private-quarantine invalidation when its persistence boundary fails, and all late shutdown rewrites use the same recovery helper; unsupported identity-bound cleanup fails closed. Every captured evidence ancestor is revalidated component-by-component without following symlinks before the report path is accepted; if interrupted-report recovery itself fails, the runner returns exit `2` without emitting the report, rather than allowing a passed report to be claimed.

The report producer rejects serialized reports larger than 1 MiB before writing stdout or a file. New reports are regular files owned by the invoking user with mode `0600`; recovery identity includes that mode. A newly created `dev-data/` entry is flushed through the repository-root descriptor before report creation continues. Quarantine directories are created and retained through no-follow descriptors, bound to the invoking user and exact mode `0700`, and their pathname is checked against the retained device/inode identity before use. Matching report bytes are cleared through the identity-checked descriptor, opened with nonblocking no-follow flags and validated as a regular file, leaving at most eight bounded empty tombstone directories per output filename; the runner never reopens or removes a quarantine pathname after the check. Existing evidence ancestors are bound to captured device/inode identities and must be invoking-user-owned, non-group/other-writable directories before a report is created or accepted at the public path; descriptor-bound recovery invalidation still removes a report when an ancestor becomes unsafe. The cleanup capability probe accepts only the kernel's expected descriptor error for a permitted `renameat2` syscall; denied or otherwise unsupported probes fail closed. If report persistence or identity-bound invalidation fails, the failure remains visible and the runner never claims that a stale report was safely removed.
Gate subprocesses receive a fresh allowlisted environment. Python manifest commands are executed through the exact interpreter that launched the runner, while the launcher directory is deliberately not added to `PATH`; this prevents a symlinked or copied virtualenv from shadowing Cargo, Git, Node, npm, or another Python executable. Cargo, cargo-clippy, cargo-deny, cargo-fmt, rustc, rustfmt, clippy-driver, Git, Node, and npm discovery prefers the first accepted inherited toolchain directory, then fixed system directories; the selected executable identities and SHA-256 digests are captured during selection, copied into independent bounded snapshots, and exposed through private launchers. Rust proxy tools are bound alongside Cargo so compiler, formatter, Clippy, and Cargo's `cargo fmt`/`cargo clippy` subcommands remain available inside the isolated namespace without exposing the ambient toolchain directory. A later pathname replacement is rejected before the copy begins, and launcher execution rechecks the source and snapshot identity/digest before `exec`. On Linux, bubblewrap places those launcher, executable, and npm-support-library directories on read-only mounts inside the gate namespace before secure fixed roots are added; the host root is read-only, host `/home` is hidden behind a private tmpfs with only the runner runtime and read-only toolchain metadata rebound, `/run`, `/tmp`, `/var/tmp`, and `/dev/shm` are replaced with private mounts so host Docker, D-Bus, SSH-agent, temporary Unix sockets, and similar runtime handles are not visible; the PID namespace receives a private procfs at `/proc` so gates cannot inspect host process environments or descriptors; the network namespace is unshared, the checkout is writable for build outputs, private HOME/TMP mounts are writable, and a private writable Cargo cache is mounted separately. Recursive contract coverage may reuse the containing namespace only when the outer runner binds a random, read-only capability file into the private `/run/voidtower-gate-capability` mount; the old caller-controlled sandbox-depth marker is not trusted and cannot disable isolation. Repository `.git` metadata is opened without following symlinks and rebound by descriptor as read-only so a gate cannot rewrite the Git index/configuration used by the post-gate cleanliness check or redirect a validated path before the namespace starts. Linked worktrees are supported only when the worktree gitdir and common `.git/worktrees/<name>` metadata are checkout-associated, the common Git directory registers the checkout as a worktree, and the pointer file, gitdir, and common directory are descriptor-bound read-only mounts; the gitdir back-pointer must match the checkout, and common metadata under `/run`, `/tmp`, `/var/tmp`, or `/dev/shm` is rejected. This is the same-UID write barrier, not a permission-only claim. Fixed roots continue to provide trusted interpreters such as `bash` and toolchain support such as `cc` without shadowing the selected identities or giving a gate a writable hard link to the source executable. Symlinked checkout directories, unsafe support-tree entries, and insecure fixed roots are rejected. Inherited `CARGO_HOME` is replaced with the private writable cache, while `RUSTUP_HOME` is retained only as a read-only toolchain source. Cleanup restores permissions throughout read-only snapshots before removing the private gate tree. A release-evidence contract test that invokes the runner recursively is recognized as an already-contained child and runs without a second bubblewrap namespace; the outer manifest gate remains the isolation boundary, preventing nested-user-namespace failures without weakening the actual gate sandbox. Repository-truth Git preflight uses the same sanitized tool environment.
Normal gate and linked-worktree Git association cleanup share the same bounded process contract. Linked-worktree Git association is validated with a trusted absolute Git executable and a fresh environment that excludes inherited `GIT_*` configuration, including caller-selected repositories, work trees, config files, and executable paths. Pointer, `commondir`, and checkout back-pointer files are regular no-follow files with a 4 KiB ceiling; bounded `git worktree list --porcelain` output has a 64 KiB ceiling. Normal and linked metadata mounts are opened component-by-component from retained checkout descriptors, no-follow, and read-only; the runner compares captured device/inode/mode/size/mtime identity before passing descriptors to bubblewrap, so replacement of the checkout or any validated metadata directory fails closed. The common directory may live outside the checkout's parent directory, but it must remain registered under the checkout-associated `.git/worktrees/<name>` topology and never reside under masked runtime or temporary roots. Association diagnostics run through the repository's parent-death-aware process supervisor; its Linux subreaper cleanup captures the supervisor start-time identity before any external cleanup, holds a pidfd for that root, and revalidates root liveness and start-time immediately before each descendant signal. A missing or changed root fails closed before descendants are signaled. A transient bounded process-snapshot (`ProcessSnapshotError`) failure is retried once within the cleanup window before supervisor termination; other cleanup failures are not retried, later escalation does not re-enter a completed cleanup retry, and any first failure still blocks the gate. Supervisor termination also uses a pidfd bound to the captured identity (or an owned-child pidfd when capture itself is unavailable), never a reusable raw PID or process-group signal; cleanup failure blocks the gate rather than claiming descendants were removed.

Trusted repository scripts are recorded by their source-owned repository-relative
argv spelling in both the runner report and publication validator; the validator
still resolves and checks the executable without following symlinks before it
accepts that canonical metadata.

The source-owned manifest also records explicitly classified `external_gates`.
These are not silently skipped product gates. Each entry is required, names its
workflow job and exact evidence filename, and must produce a passed
`release_external_gate` record bound to the candidate commit. The current
external gates are the Docker/restic golden-path runtime job and repository-host
secret-history scanning. The publication validator rejects missing, unexpected,
non-passed, mismatched-commit, mismatched-job, or mismatched-filename evidence;
the release job cannot publish until both jobs and the `build` matrix complete.
The CI workflow has a dedicated `release-gates` job that installs the same
source-owned Python, Node, Rust, Cargo-deny, and standalone-MCP prerequisites
and invokes `scripts/release_gate.py --scope all --require-clean` against the
same manifest. The ordinary CI jobs remain useful diagnostics, but they do not
replace this manifest-driven parity job. The source-owned manifest includes a separate required `schema-migration-ownership` gate, so migration numbering/immutability is exercised by the same clean-candidate command rather than left as an operator-only prerequisite.
The golden-path evidence must also include the SHA-256 of the exact x86_64
release archive downloaded from the build matrix. The workflow moves that exact
archive into the job root, uses `scripts/extract_release_binary.py` to reject
traversal, links, special files, duplicate members, and bounded-size violations
before extracting only the regular `voidtower` member, and sets
`VOIDTOWER_REQUIRE_PACKAGED=1`. The runtime harness therefore launches the
`voidtower` binary extracted from that exact archive. On Unix it opens every
packaged parent directory and the final binary through no-follow descriptors,
retains those identity-bound descriptors through process spawn, and executes
through the retained binary descriptor; a symlink or replacement parent fails
closed. If the packaged path exists but is unsafe, the harness fails closed in
both strict and local modes; only a genuinely missing package may use the
`descriptor-bound debug fallback`. The validator opens each archive once through
a retained no-follow descriptor, hashes that descriptor for report and external
evidence checks, and then creates a descriptor-bound hard-link set under
`--publication-root`; the validator rehashes each retained descriptor and
removes write permission before returning the read-only handoff. The publish
workflow attests and uploads only that retained set, including the validated
release-gate and external-evidence report snapshots, so checksum, attestation, and upload
steps keep the exact archive/report inode and verified content. Publication roots are
checkout-contained and walked component-by-component without following symlinks. Source-built
runtime checks are not a substitute for this archive binding.
When an external job uses `--observed-file`, the evidence record stores only the
relative marker name, byte count, and SHA-256 digest. It never copies marker
contents into a release artifact, and the publication validator rejects legacy
or forged `observed_content_b64` fields and unknown extra fields. The helper
reads the marker through one bounded no-follow file descriptor to avoid a
symlink/replacement race while hashing. Artifact, marker, and command working
directories are opened through POSIX no-follow directory handles. A marker therefore proves the bounded
step identity without turning the evidence bundle into a data-exfiltration path.
The golden-path workflow creates its `RESTIC_PASSWORD` through Python's
cryptographic random generator immediately before the runtime command, masks
the value in workflow logs, and passes it only to the golden-path process. The
CI workflow passes it directly to Cargo; the release workflow passes it to the
evidence helper and its golden-path child process. The repository does not
carry a reusable restic credential in workflow configuration, and the
generated value is not included in release evidence.

## Exit statuses

- `0`: every selected required gate passed and no required gate was skipped.
- `1`: the runner completed, but a required gate failed, was blocked, or was skipped, the clean-check preflight rejected a dirty checkout, or the runner received an interrupt before the report finalization commit (including after the final gate). The final retained-report descriptor close is the commit boundary; signals arriving after that close do not rewrite an already committed report.
- `2`: the runner could not safely load the repository, manifest, or output path, or rejected an unsafe gate declaration.

A `passed` source or unit gate is not runtime or release qualification. Promote evidence only when the applicable runtime, artifact, installation, upgrade, recovery, and named-platform checks have also run successfully.

## Compose readiness and App Vault coexistence

The root `docker-compose.yml` keeps the public UI listeners on host ports 80 and
443 and preserves the existing 8745 publication. It publishes nginx's port 80
on loopback host ports 8000 and 8743. Port 8000 exists for generic Hermes
startup verification; strict readiness must request
`http://127.0.0.1:8743/api/health`, which exercises the same backend health
boundary used by the container healthcheck; an HTTP response from `/` alone is
not health evidence. The Paperless App Vault manifest uses host port 8001 for
its container port 8000, so the two stacks can coexist without a bind collision.
This is an intentional host-port migration for direct Paperless access: existing
Paperless deployments must be recreated/redeployed from the updated manifest;
the named Paperless data volumes are not changed. After migration, use host port
8001 for direct Paperless access.
Hermes's generic Compose autodetection defaults to port 8000 and probes `/`; it
is startup/proxy evidence only, not a release health assertion.

Before starting the root stack after upgrading from the old Paperless manifest,
run the read-only migration preflight and resolve any reported legacy container:

```sh
PYTHONDONTWRITEBYTECODE=1 python3 scripts/compose_preflight.py
docker compose up -d
```

The preflight fails closed when a running container still publishes host port
8000 to container port 8000. It does not stop, recreate, or delete containers;
the operator must explicitly recreate Paperless from its updated App Vault
manifest, then rerun the check.

Validate the effective mapping and healthcheck contract with:

```sh
PYTHONDONTWRITEBYTECODE=1 python3 -m unittest scripts.test_compose_contract -v
docker compose config --quiet
curl --fail --silent http://127.0.0.1:8743/api/health
```

The Compose contract test and a successful health response establish only
startup/readiness evidence. They do not establish completed first-run setup,
normal-operation, artifact, installation, upgrade, recovery, browser, or
release qualification.

## Manifest safety contract

Gate entries use explicit `argv` arrays and repository-relative working directories. Shell interpreters, absolute executables, unallowlisted repository executables, inline interpreter code, duplicate IDs, invalid timeouts, and paths escaping the checkout are rejected. Gate stdout and stderr are concurrently drained and retained only up to the bounded diagnostic limit. Each gate runs through `scripts/process_supervisor.py`: the supervisor and its command share a process group, Linux parent-death signaling handles runner disappearance, and the Linux child-subreaper tree cleanup also covers descendants that create a new session. Process-tree snapshots are bounded and retry malformed or unreadable `/proc/<pid>/stat` identity data a fixed number of times before failing closed; only explicitly observed adopted descendants are reaped under a fixed budget, while the supervisor's direct gate child remains owned by its `Popen` wait. The release runner independently performs bounded descendant signaling without reaping the supervisor's direct child, and treats cleanup failure or supervisor exit status 125 as a gate error before terminating the supervisor. Timeout and process-group cleanup therefore remain fail closed. Credential-like command arguments and diagnostics are redacted before they enter the report. Repository gates receive a private tool path, private temporary home, locale, and derived Rust toolchain cache paths; selected Cargo, Git, Node, and npm identities are bound to private snapshots and launchers that verify source/bound identity and fail closed if they disappear or change, while a private support overlay exposes secure fixed-root tools such as `bash` and `cc` without retaining a fallback for any selected-tool name. Runner credentials, package configuration paths, `PYTHONPATH`, Git repository/configuration overrides, and arbitrary ambient variables are not inherited. The process-supervisor path is currently Linux-specific; non-Linux qualification is not claimed by this slice.

The manifest is source-owned. Add or change a gate only when its subsystem, required status, timeout, bounded diagnostics, and artifact evidence are part of the tracked release plan. The runner is intentionally not an artifact uploader or publisher.

## Action-schema catalog freshness

`backend/src/action_registry.rs` is the source of truth for durable action schema identities. Regenerate the bounded catalog with:

```sh
PYTHONDONTWRITEBYTECODE=1 python3 scripts/generate-action-schema-catalog.py
```

Before relying on a release report, require the byte-for-byte freshness check and catalog contract suite:

```sh
PYTHONDONTWRITEBYTECODE=1 python3 scripts/generate-action-schema-catalog.py --check
PYTHONDONTWRITEBYTECODE=1 python3 -m unittest scripts.test_action_schema_catalog -v
```

The checker fails closed when the catalog's source provenance hash or any bounded v1 schema identity differs from the registry. A matching action count alone is not sufficient evidence.

## Generated API contract freshness

`backend/contracts/api-v1-envelope-contract.json` is the source-owned versioned API/OpenAPI contract. `scripts/generate-api-contract.mjs` renders `frontend/src/api/generatedApiContract.ts` and `docs/api-v1.openapi.json` from it. Regenerate with:

```sh
node scripts/generate-api-contract.mjs
```

Two checks enforce freshness at different fidelity:

```sh
# Byte-exact generator check; runs in the CI frontend job where Node is provisioned.
cd frontend && npm run contracts:check

# Toolchain-independent drift gate; required in scripts/release-gates.json.
PYTHONDONTWRITEBYTECODE=1 python3 -m unittest scripts.test_api_contract_drift -v
```

The required `api-contract-drift` gate runs `scripts/api_contract_drift.py` through its contract suite, so it stays executable in the release runner's isolated namespace without Node. It re-derives the source SHA-256, compares the embedded client contract payload with the source artifact, and compares the published OpenAPI document with the source `openapi` member; a stale digest, stale payload, semantically edited OpenAPI document, or unreadable generated artifact fails closed. Because it compares parsed content, a whitespace-only rewrite passes this gate and is caught only by the byte-exact generator check. The generator remains the byte-exact authority, but the gate cannot prove the OpenAPI document is derived from Axum route metadata rather than authored in the contract artifact, and it is not runtime, browser, or release qualification. The gate executes a `unittest` module, so like every Python gate in this manifest it cannot detect its own test module being emptied or skipped; the CI byte-exact step and review remain the boundary for that.

Before the GitHub release publisher creates a release, it runs:

```sh
python3 scripts/validate_release_evidence.py \
  --root artifacts \
  --manifest scripts/release-gates.json \
  --expected-reports 2 \
  --expected-commit "$GITHUB_SHA" \
  --publication-root published-artifacts \
  --checksums-output SHA256SUMS
```

The validator requires each report's executable command and working directory,
and its external gate descriptors, to match the source manifest. It requires
one passed external evidence record for every declared
external gate, one passed report per produced architecture archive, exact
root-level archive and report placement, duplicate/unmatched archive rejection,
safe report paths and digests, and recomputed SHA-256 values from every
downloaded archive. Each report must bind to the full lowercase commit checked
out by the publish job, include non-empty machine/system/release metadata, and
include each passed gate's bounded printable redacted argv, repository-relative
cwd, and zero exit code. The publication boundary independently revalidates the
manifest executable allowlist (including shell/interpreter restrictions and
repository-contained arguments), rejects collisions between executable and
external gate IDs, and requires `selected_subsystems` to equal the sorted union
of the manifest's executable gate subsystems. It accepts only the checkout's
source-owned `scripts/release-gates.json`, canonicalizes trusted repository
script paths exactly as the runner records them, and requires those scripts to
be regular executable files without symlink traversal. Both runner and
publication validator reject POSIX, Windows-drive, rooted-backslash, and
backslash-traversal forms in gate command arguments and working directories.
The external-evidence helper and publication validator apply the same boundary
to cwd and observed-file metadata, so a relative path cannot redirect a
release record outside its checkout. The secret-history workflow records the
successful action marker without passing a second command to the helper; the
manifest describes this as the `observed-ci-step` contract, avoiding an
unavailable synthetic executable after the scanner action has completed.
Both workflow jobs pin checkout to `${{ github.sha }}`. The build job has
`contents: read`; only the publish job has `contents: write`. A mismatch or
incomplete execution/provenance record stops publication before checksums are
generated or the GitHub release action runs. The validator is covered by
`scripts/test_release_evidence.py` and is also a required
`release-evidence-contracts` manifest gate.

Publication also requires each passed report to describe a clean candidate:
both `git.staged_paths` and `git.modified_paths` must be present, typed arrays
and empty. The downloaded report is a closed v1 top-level contract; unknown
fields are rejected rather than carried into the release evidence set. The
nested v1 contract is closed as well: report `git` and `platform` objects,
gate results and their artifact records are bound to the manifest declaration,
manifest gate/external descriptors, and external-evidence platform metadata reject
unknown fields and malformed values. Git path lists and gate artifact paths must
remain bounded repository-relative metadata. This keeps future producer drift
fail-closed at both the release runner and publication boundaries instead of
allowing unreviewed metadata into release evidence. The
two-report release matrix is exact: one archive named
`voidtower-<version>-x86_64-unknown-linux-musl.tar.gz` and one named
`voidtower-<version>-aarch64-unknown-linux-musl.tar.gz`, using the same
`<version>`. Missing, unexpected, duplicate, or mixed-version targets fail
before checksums or publication. The runner and validator also reject final or
parent symlinks for artifacts instead of hashing or publishing through them.
Manifest, release-report, and external-evidence JSON are loaded through bounded
regular-file descriptors with no-follow protection on the immediate parent and
file. A symlink or non-regular replacement at either boundary therefore fails
closed during the read rather than relying only on the earlier directory scan;
each report is parsed from one bounded descriptor read, and platforms without
the required safe primitives are not qualified.
Explicit and manifest-declared artifacts are hashed before and after gate
execution; an existing artifact that changes during a gate is rejected rather
than allowing the post-gate bytes to redefine the release evidence. This is a
completed gate failure, not a malformed invocation: when an artifact changes,
the runner records a bounded failed gate/report (including the artifact error),
persists it when `--output` is supplied, and exits `1`. Unsafe artifact paths
and unreadable evidence destinations remain invocation errors with exit `2`.
The integrity failure is sticky across every declared artifact hash in that
gate, including a later missing or unreadable artifact; an optional gate cannot
turn a prior artifact-integrity failure into a green top-level report. The
publication validator recognizes `artifact_error` as a bounded failed-report
diagnostic but rejects that diagnostic on any passed report.
The publication validator retains each archive through a no-follow regular-file
descriptor, rehashes every retained descriptor after report and external-gate
validation even when no publication directory is requested, and fails closed
when the retained bytes changed during validation. When publication is
requested, each output is copied from that retained descriptor into an independent
unnamed snapshot, fsynced,
rehash-checked, and made read-only before the snapshot is linked into the
publication directory. The freshly-created directory is made non-writable
before any destination pathname is opened for its matching snapshot
device/inode identity and digest checks. This independent snapshot also
prevents a writer that already held the downloaded archive open from changing
the published bytes after validation; a later pathname replacement cannot
silently substitute same-content bytes during checksum or upload steps.
Published archives are bounded to 512 MiB at descriptor open, every retained
hash read, and every snapshot copy; sparse or concurrently-growing archives
therefore fail closed before consuming unbounded I/O or disk. The publication
command writes `SHA256SUMS` from the retained descriptors, fsyncs and seals it,
then removes write permission from the checkout directory before returning.
The evidence tree is walked through the retained root descriptor, and report
and external-evidence files are replaced with fsynced read-only snapshots of
the bytes and inode identities that were validated earlier. This replacement
means a writer that already held an evidence-file descriptor cannot change the
pathname consumed by downstream upload. Publication roots must be direct
checkout children; nested writable parents are rejected, and the checkout plus
its immediate parent are sealed before the pathname handoff returns.
Even when no publication root is requested, the validator re-reads every
retained report and external-evidence descriptor before returning success; a
same-inode mutation or replacement of the validated pathname therefore fails
closed instead of producing an apparently valid pre-publication result. Report
revalidation remains bounded to 4 MiB, while external-gate evidence keeps its
64 KiB bound throughout the retained-descriptor check.
The later attestation and upload steps consequently consume a read-only,
descriptor-validated pathname handoff rather than reopening a writable parent.
The runner checks its shutdown flag immediately before final status calculation,
after final artifact hashing, and after report persistence. An interrupt in the
artifact or persistence phase rewrites the report as `failed` and exits `1`;
there is no passed-report exception for a late signal.
The release workflow keeps the unpacked package and checksum list under the
ignored `dist/` directory while leaving the archive at the repository root;
this prevents packaging outputs from making the candidate dirty before the
runner records its report. Python manifest gates execute through the exact
interpreter that launched the runner, preserving a virtualenv or other
symlinked launcher's dependency closure without putting that launcher's
directory on `PATH`. The allowlisted `PATH` therefore cannot let a sibling
`python3`, `cargo`, `npm`, or `git` shadow the trusted tool directories. The
gate environment still excludes ambient package and credential configuration.

## Repository prerequisite gates

Run the governance prerequisites directly before relying on a release report:

```sh
bash scripts/check-repository-hygiene.sh
bash scripts/check-schema-migration-ownership.sh
python3 -m unittest scripts.test_repository_prerequisites -v
```

Repository hygiene intentionally allows tracked continuity evidence under
`docs/internal/`, while still rejecting tracked credentials, private keys,
generated build output, local machine state, and all tracked symlinks. Git paths
are read with NUL-safe records so unusual filenames cannot bypass the policy.
The migration ownership gate does not depend on `rg` or another optional search
utility: it scans Rust `sqlx` query/query-as/query-scalar/raw-SQL calls,
including comment-separated paths, turbofish, and macro forms, conservatively
rejects the complete `query_file_*` macro family,
excludes only the documented legacy adopter, requires contiguous numbered SQL
migrations starting at `0001`, rejects migration/source symlinks and paths that
escape the repository, and rejects migrations that are not tracked by Git. A
missing tool or malformed migration layout must fail the gate rather than print
a success line.

These checks establish repository/source governance only. The release manifest
declares both `backend-clippy` and `backend-format`; a passing backend quality
gate therefore requires strict Clippy and `cargo fmt --all -- --check` in the
same evidence run. CI installs and qualifies `cargo-deny@0.20.2` because current
RustSec advisories use CVSS 4.0 metadata that older cargo-deny releases cannot
parse. Rust lint components, frontend dependencies, a host supervisor, Docker,
a browser, and a packaged runtime remain explicit prerequisites in the
machine-readable report.

## G5 provider plaintext scan

G5 requires that database and log scans find no provider plaintext. Deleting a credential row does
not erase its bytes from the SQLite file, so the scan reads raw bytes rather than live rows.

The required gate runs the scan contract suite:

```sh
PYTHONDONTWRITEBYTECODE=1 python3 -m unittest scripts.test_g5_plaintext_scan -v
```

The suite proves the scanner detects provider-shaped residue in a database file and in its `-wal`
sidecar, detects a deleted Slack incoming-webhook URL and a deleted Proxmox API token, detects
plaintext in an application log, classifies a retained `*.pre-migration-v1-*.bak` snapshot through
the same `scan_candidate` seam an operator's CLI run uses, reports every authoritative legacy
plaintext `settings` key that is still readable through SQL and an `ai_providers.api_key_ref` that
still names a settings row, states the classes no byte pattern covers in `coverage_limits`, stays
silent on a migrated WAL-mode database whose secrets are ciphertext, proves that no provider pattern
matches the product's own redaction forms, never echoes a matched value, leaves the scanned database
file's content and mtime unchanged while pinning the empty `-shm`/`-wal` sidecars a read-only
connection creates, fails closed on oversized, non-regular, or unreadable input, on an explicitly
named database with no `settings` table or with a `settings` table that is not `key`/`value`, on a
partial (`--db`-only or `--log`-only) invocation, and on a single match longer than the bounded
carry, and is declared `required` in `scripts/release-gates.json`. A contract test also pins the
complete required gate id set, so adding, removing, or demoting any gate is a visible reviewed
change rather than an unnoticed edit. The suite executes the same `unittest` module the manifest
declares, so — like every Python gate here — it cannot detect its own module being emptied.

The legacy key set is not a free-standing list. A drift test parses `backend/src/api/secrets.rs` for
the `legacy_setting` fields, the `*_LEGACY_SETTING` constants, and literal keys addressed inside
`migrate_legacy_*` functions, then fails when the scanner's tuple and that source disagree in either
direction. AI-provider legacy keys are dynamic — they come from `ai_providers.api_key_ref` — so the
live-row check finds them instead of a list trying to enumerate them.

### What the byte patterns do not cover

Byte patterns exist only for values whose shape the provider or the product defines: OpenAI keys,
Google API keys, Slack `xox*` tokens, Slack incoming-webhook URLs, Discord webhook URLs, Proxmox API
tokens in the documented `user@realm!tokenid=uuid` form, PEM private-key blocks, and `Bearer` values.

Two authoritative legacy classes have no discoverable shape: the ntfy topic URL (`notif_ntfy_url`,
an arbitrary self-hosted URL and topic) and the Odysseus webhook secret (`odysseus.webhook_secret`,
48 hex characters, a width shared by unrelated identifiers, so a pattern would also match values that
are not credentials). The live-row check reports both while
their settings row remains. The one-time plaintext-residue rebuild in `docs/recovery.md` is the
control for copies deleted before it ran. A copy deleted *after* the rebuild is outside byte-pattern
coverage, so a clean scan is not proof that those two classes are absent; every JSON report names
them in `coverage_limits` and the human-readable output prints the same list.

### Running the scan against a candidate

The container and systemd installs write the application log to different sinks, so export it to a
file first. The systemd unit sends both streams to the journal:

```sh
journalctl -u voidtower --no-pager -o cat > /tmp/voidtower.log
# Docker install instead: docker logs voidtower > /tmp/voidtower.log 2>&1

PYTHONDONTWRITEBYTECODE=1 python3 scripts/g5_plaintext_scan.py \
  --db /var/lib/voidtower/voidtower.db \
  --log /tmp/voidtower.log \
  --json
```

Both `--db` and `--log` are required: a G5 result needs the database and the log, and a partial
invocation exits `2` rather than reporting a clean scan.

Exit status is `0` for a clean scan, `1` when any finding exists, and `2` when no clean result can be
claimed: an input is missing, unreadable, not a regular file, or larger than the 512 MiB bound, an
explicitly named `--db` has no readable `settings(key, value)` table, or a single credential-shaped
match runs past the bounded carry. A database is identified only by that settings table, so a
foreign database with an identical shape cannot be told apart — name the intended file. Findings
carry a kind (`residue`, `log`, `snapshot`, `live_plaintext_setting`), a file name, a pattern name,
and — where the finding has one — a byte offset (`null` for live-row and finding-limit findings),
never the matched value, so a report can be attached to an evidence bundle without copying a
credential into it.

The scan is read-only with respect to content: the scanned database file's bytes and mtime are
unchanged. Opening a WAL-mode database through SQLite with `mode=ro` can create its `-shm`
shared-memory index and an empty `-wal` sidecar; when a writer already holds an uncheckpointed WAL,
the wal-index read-mark slot in an existing `-shm` can be rewritten. Every sidecar is scanned before
that connection opens, none carries database content, and both are accepted for an evidence run.
Record the directory listing before the scan when the bundle must show that nothing else appeared.

This gate is `unit-verified` enforcement only. It does not produce G5 release evidence: the scan must
still be run against the candidate database and log of the produced artifact, and a retained
pre-migration snapshot that still contains pre-migration plaintext remains an open operator retention
decision rather than a pass. Per-event `webhook_configs.url` destinations remain plaintext by design
at the compatibility approval boundary; this gate does **not** scan or report them, and handoff
`0059` remains the open block for that class.

## Verification

The focused contract suites are:

```sh
python3 scripts/test_release_gate.py -v
python3 -m unittest scripts.test_g5_plaintext_scan -v
python3 scripts/test_mcp_dependencies.py -v
python3 scripts/test_repo_truth.py -v
python3 scripts/repo_truth.py --repo . --json --check
```

The first suite covers manifest validation, explicit-argument execution, redaction, bounded output, timeout behavior, mandatory-skip failure, repository-contained evidence, nested subprocess compatibility, and runner-death cleanup for forked descendants. The release-evidence suite covers archive/report parity, digest mismatch, duplicate archive names, unsafe report paths, external observed-marker confidentiality, legacy embedded-content rejection, and workflow wiring. The repository-truth suite covers deterministic source inventory, Git status/range handling, symlink and output bounds, and credential-safe diagnostics. The prerequisite suite covers NUL-safe hygiene, tracked symlinks, credential-like continuity paths, SQLx DDL call variants, query-file rejection, canonical-path containment, migration numbering, and missing optional tools.

The full release run remains environment-dependent. Missing `cargo-deny`, unavailable Rust lint components, forbidden tracked historical internal paths, missing frontend dependencies, or unavailable host/runtime services are recorded as blockers; they must not be silently waived or represented as release-qualified evidence. When standalone-MCP dependencies are available only in a pre-existing local environment, use `PYTHONPATH` only for the direct contract-suite command; do not treat that invocation as evidence that the isolated release runner can execute the gate, and do not install or download dependencies from the release-gate runner. A full manifest that fails `backend-format` on preserved unrelated paths is blocked until the owning workstream reconciles those paths; do not format or stage them as part of an unrelated release-gate recheck.
