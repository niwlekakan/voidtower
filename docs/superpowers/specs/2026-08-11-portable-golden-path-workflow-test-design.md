# Portable Golden-Path Workflow Test Design

## Context

`backend/tests/golden_path.rs` is a hermetic acceptance test for the real-infrastructure job in `.github/workflows/ci.yml`. Ordinary Cargo runs execute it from the checked-out `backend/` crate, where the workflow is one directory above `CARGO_MANIFEST_DIR`.

Cargo-mutants instead copies the crate into a temporary directory. The repository-level `.github/` directory is not part of that copy, so the unmutated baseline currently fails before mutation testing starts. GitHub Actions still exposes the original checkout through `GITHUB_WORKSPACE`.

## Decision

The test will resolve the workflow path from `GITHUB_WORKSPACE` when that environment variable is present. Outside GitHub Actions it will retain the existing `CARGO_MANIFEST_DIR` parent fallback.

Both paths point to the real workflow for their environment. Reading, YAML parsing, trigger checks, job checks, dependency checks, and harness-command checks remain strict. A missing workspace file is an error, not a reason to skip.

## Rejected alternatives

- A copied workflow fixture under `backend/` could drift from the deployed workflow and produce false confidence.
- Excluding the test from mutation runs would weaken the baseline and conceal future test incompatibilities.
- Changing the mutation job to run in place would modify an existing forbidden workflow job and reduce mutation isolation.

## Implementation plan

1. Add a failing unit regression that passes a simulated cargo-mutants manifest path and a GitHub workspace path to a workflow-path resolver.
2. Extract the resolver and use it in the existing acceptance test.
3. Run the targeted tests and repository gates.
4. Verify the mutation baseline in GitHub Actions after merge.

## Scope and risk

Only the hermetic test path resolver changes. Production code, workflow configuration, mutation targets, dependencies, and the real Docker/App Vault/restic harness remain unchanged.
