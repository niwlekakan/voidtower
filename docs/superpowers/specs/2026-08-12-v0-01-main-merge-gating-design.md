# V0-01 Main Merge-Gating Design

**Status:** Approved under the operator's standing auto-approval on 2026-08-12

**Roadmap item:** V0-01 — Complete golden-path CI

**Repository:** `niwlekakan/voidtower`

## Problem

VoidTower already runs real Docker/App Vault, container lifecycle, and restic backup/restore
coverage in the `Golden path (Docker + App Vault + restic)` GitHub Actions job. The workflow runs
for pull requests and pushes to `main`, but GitHub reports that `main` has neither branch
protection nor a repository ruleset. A pull request can therefore merge while the golden path is
pending or failing. V0-01's merge-gating acceptance condition is not satisfied by the workflow's
presence alone.

## Decision

Protect `main` with GitHub's branch-protection API and require the following GitHub Actions check
contexts to succeed before merge:

1. `Frontend (lint + build)`
2. `Backend (clippy + test)`
3. `Supply chain (cargo-deny + cargo-audit)`
4. `Mutation testing (policy + voidwatch)`
5. `Golden path (Docker + App Vault + restic)`
6. `Build & push image`

The protection uses strict status checks so pull requests must be current with `main`. Pull
requests are mandatory, conversation resolution is mandatory, and force-pushes and branch
deletion are disabled. The policy applies to administrators. It does not require another person's
approval because VoidTower currently has one active maintainer; requiring one would make ordinary
maintenance impossible. Administrators may still edit the protection rule through an explicit
repository-administration operation when emergency recovery is necessary.

This is an external repository-policy change. It does not alter workflow code, application code,
secrets, environments, or deployment credentials.

## Alternatives Considered

### Require only the golden-path context

This narrowly satisfies the named V0-01 acceptance line, but would still permit merging code that
fails compilation, frontend validation, dependency security checks, mutation testing, or image
construction. It is inconsistent with treating the entire 1.0 verification pipeline as release
infrastructure.

### Add an aggregate `ci-gate` job

One stable required context would reduce branch-protection churn when job names change. It would
also require a forbidden-zone workflow edit and another authorization/review cycle without adding
validation. The direct six-context rule is preferable now. An aggregate job can replace it later
if workflow renames become frequent.

### Use organization-level required-workflow rulesets

Rulesets are valuable when the same policy spans multiple repositories. VoidTower currently has no
repository or organization rulesets, and this single-repository policy does not justify that extra
control plane.

## Enforcement Contract

- Every change to `main` arrives through a pull request.
- All six required contexts must report success on the pull request head.
- The head must be current with `main` before merge.
- Unresolved review conversations block merge.
- Administrators are subject to the same merge conditions.
- Force-push and deletion of `main` are rejected.
- Signed commits, linear history, merge queue, deployments, and code-owner approval are not added
  by this task.

The no-approval choice is deliberate. Review quality continues to come from the repository's
two-axis agent review and local gates, while GitHub prevents a solo-owner deadlock. A later team
governance task may require independent human or code-owner approval when another eligible
maintainer exists.

## Application and Verification

Apply the complete protection document with one idempotent `PUT` request to
`/repos/niwlekakan/voidtower/branches/main/protection`. Read it back immediately and assert:

- strict status checks are enabled;
- the required-context set equals the six names above;
- pull requests and conversation resolution are required;
- administrator enforcement is enabled;
- force-push and deletion are disabled.

The repository records the intended policy and recovery commands in an operator document. The
read-back response is the authoritative runtime evidence; documentation alone is not acceptance.

## Failure and Recovery

If GitHub rejects a context because a workflow job was renamed, keep `main` protected and update
the context set only after confirming the replacement job has run successfully on a pull request.
Do not temporarily remove all required checks.

If a required GitHub service is unavailable, leave the pull request open. For an emergency where
repository administration itself is the only recovery path, the owner may explicitly edit the
rule, document the reason, restore the six-context policy immediately afterward, and verify the
read-back state again.

## Acceptance

V0-01 is complete when:

- the real golden-path job remains present on pull requests and pushes to `main`;
- GitHub returns an active protection rule for `main` containing exactly the six required checks;
- administrators cannot merge a pull request with a pending or failing required check through the
  normal merge path;
- the repository contains an operator-facing policy and recovery record; and
- no application, workflow, credential, or unrelated user file changed.
