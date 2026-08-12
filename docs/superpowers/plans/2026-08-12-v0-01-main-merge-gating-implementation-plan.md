# V0-01 Main Merge-Gating Implementation Plan

**Design:** `docs/superpowers/specs/2026-08-12-v0-01-main-merge-gating-design.md`

**Roadmap item:** V0-01 — Complete golden-path CI

## Goal

Make the existing full GitHub Actions validation suite, including the real Docker/App
Vault/restic golden path, a mandatory condition for merging into `main`.

## Step 1 — Capture the baseline

1. Read `.github/workflows/ci.yml` and `.github/workflows/docker.yml` to verify the six job names.
2. Query branch protection and repository rulesets.
3. Confirm that `main` is currently unprotected and that each required context has been emitted by
   GitHub Actions.

**Evidence:** the branch-protection endpoint returns `404 Branch not protected`, rulesets are
empty, and check-run inventory contains all six selected names.

## Step 2 — Apply the complete protection document

Send one `PUT /repos/niwlekakan/voidtower/branches/main/protection` request with:

- strict required checks containing the exact six-context set;
- pull-request requirement enabled with zero approvals;
- administrator enforcement enabled;
- required conversation resolution enabled;
- force-push and branch deletion disabled;
- no restrictions, signed-commit requirement, linear-history requirement, deployment gate, or
  merge queue added by this task.

Do not perform incremental partial updates that can leave `main` in an ambiguous state.

## Step 3 — Verify the runtime policy

Read the protection endpoint back and machine-check every design invariant. A mismatch is a failed
implementation even if GitHub returned a successful mutation response. Reapply the full document
if necessary; never clear all required checks as a repair shortcut.

## Step 4 — Record operator guidance

Add `docs/operations/github-merge-gates.md` containing:

- the required-context inventory;
- how to inspect current protection and PR check state;
- the safe job-rename procedure;
- the explicit emergency recovery and restoration procedure; and
- the solo-maintainer rationale for zero required approvals.

## Step 5 — Prove enforcement with this branch

1. Run repository gates for the documentation diff.
2. Push `devteam/V0-01-merge-gating` and open a pull request.
3. Verify GitHub reports all six contexts as required and the PR as blocked while any is pending.
4. Wait for all six contexts to succeed.
5. Complete standards/spec review and merge through the ordinary protected path without an
   administrator bypass.
6. Read branch protection back once more after merge.

## Acceptance Checklist

- [ ] Exact six-context set is required with strict updates.
- [ ] Pull requests and conversation resolution are required.
- [ ] Administrator enforcement is enabled.
- [ ] Force-push and deletion are disabled.
- [ ] Operator documentation is committed.
- [ ] The proof PR cannot merge while a required context is pending or failing.
- [ ] The proof PR merges normally after all required contexts succeed.
- [ ] Unrelated untracked files remain untouched.
