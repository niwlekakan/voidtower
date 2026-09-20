# Release-candidate gate runner

VoidTower's release-candidate evidence collector is `scripts/release_gate.py`. It executes the repository-owned manifest at `scripts/release-gates.json` and writes a machine-readable report. The runner itself does not publish artifacts, stage or commit changes, or read environment files; declared gates may create their normal build and test outputs.

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
  --json \
  --output dev-data/release-gate.json
```

`--scope changed` selects governance gates plus gates whose declared subsystem appears in the source-truth changed-subsystem inventory. Required gates that are skipped because their subsystem is unchanged are recorded in `skipped_gates` and make the report fail closed. Use `--scope all` when the evidence must cover the complete candidate.

The output report contains the manifest schema version, Git state, platform information, selected subsystems, each gate's redacted explicit argument vector and bounded diagnostics, skipped gates, and SHA-256 hashes for declared artifacts. The output path must remain inside the repository.

## Exit statuses

- `0`: every selected required gate passed and no required gate was skipped.
- `1`: the runner completed, but a required gate failed, was blocked, or was skipped. Unavailable gate tools and services are recorded in this report status.
- `2`: the runner could not safely load the repository, manifest, or output path, or rejected an unsafe gate declaration.

A `passed` source or unit gate is not runtime or release qualification. Promote evidence only when the applicable runtime, artifact, installation, upgrade, recovery, and named-platform checks have also run successfully.

## Manifest safety contract

Gate entries use explicit `argv` arrays and repository-relative working directories. Shell interpreters, absolute executables, unallowlisted repository executables, inline interpreter code, duplicate IDs, invalid timeouts, and paths escaping the checkout are rejected. Gate stdout and stderr are concurrently drained and retained only up to the bounded diagnostic limit. Each gate runs through `scripts/process_supervisor.py`: the supervisor and its command share a process group, Linux parent-death signaling handles runner disappearance, and the Linux child-subreaper tree cleanup also covers descendants that create a new session. Timeout and process-group cleanup therefore remain fail closed. Credential-like command arguments and diagnostics are redacted before they enter the report. The process-supervisor path is currently Linux-specific; non-Linux qualification is not claimed by this slice.

The manifest is source-owned. Add or change a gate only when its subsystem, required status, timeout, bounded diagnostics, and artifact evidence are part of the tracked release plan. The runner is intentionally not an artifact uploader or publisher.

## Verification

The focused contract suites are:

```sh
python3 scripts/test_release_gate.py -v
python3 scripts/test_repo_truth.py -v
python3 scripts/repo_truth.py --repo . --json --check
```

The first suite covers manifest validation, explicit-argument execution, redaction, bounded output, timeout behavior, mandatory-skip failure, repository-contained evidence, nested subprocess compatibility, and runner-death cleanup for forked descendants. The repository-truth suite covers deterministic source inventory, Git status/range handling, symlink and output bounds, and credential-safe diagnostics.

The full release run remains environment-dependent. Missing `cargo-deny`, unavailable Rust lint components, forbidden tracked historical internal paths, missing frontend dependencies, or unavailable host/runtime services are recorded as blockers; they must not be silently waived or represented as release-qualified evidence.
