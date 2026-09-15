# Release-candidate gate runner

`python3 scripts/release_gate.py --repo . --manifest scripts/release-gates.json --output evidence/release-gate.json --json`

The runner is a read-only evidence collector. It derives the current Git state and changed subsystems from `scripts/repo_truth.py`, selects governance gates plus gates for changed subsystems, and emits a JSON report on stdout. `--output` additionally persists the exact report at a repository-contained path (the parent directory is created if needed). Use `--scope all` to execute every declared gate for a release-candidate attempt.

Each gate is declared in `scripts/release-gates.json` with an ID, subsystem, repository-relative working directory, explicit argument vector, timeout, and optional repository-contained artifact paths. Executable names are either approved tool names (`python3`, `cargo`, `npm`, `git`) or existing repository-relative scripts; absolute executables, path/symlink escapes, shell interpreters, and inline interpreter code are rejected. Commands are executed with `shell=False`. Gate output is bounded and common credential-like assignments, structured credential fields, URL parameters, command-line values, and bearer values are redacted. Artifact digests use SHA-256 and are reported only after the gate run.

Exit status:

- `0`: every selected required gate passed and no required gate was skipped. This is not runtime or release qualification by itself.
- `1`: a selected required gate failed or was blocked by timeout/startup/artifact evidence.
- `2`: the checkout or manifest could not be validated safely.

The report records selected and explicitly skipped gates. A skipped gate is not evidence that its check passed: in `changed` scope, any skipped `required: true` gate makes the report and process fail. Use `--scope all` for a complete candidate attempt. Missing tools, unavailable services, failing checks, and host-only requirements remain blocked in the report. The runner does not install dependencies, change source, stage or commit files, publish artifacts, or read `.env`/credential files.

The checked-in manifest declares the repository's current CI-facing governance, schema, backend, frontend, standalone-MCP, supply-chain, and diff checks. Some gates intentionally require tools or services not available in the development sandbox; use their recorded result rather than promoting a local report to `release-qualified`. Installation, upgrade, rollback, and host-runtime evidence still require the named supported environment and are not created by this runner.
