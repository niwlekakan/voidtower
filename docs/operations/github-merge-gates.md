# GitHub Merge Gates

VoidTower's `main` branch is protected by the desired-state document in
`docs/operations/github-main-protection.json`. V0-01 made the existing verification pipeline
merge-gating on 2026-08-12.

## Required checks

Every pull request must be current with `main` and pass:

- `Frontend (lint + build)`
- `Backend (clippy + test)`
- `Supply chain (cargo-deny + cargo-audit)`
- `Mutation testing (policy + voidwatch)`
- `Golden path (Docker + App Vault + restic)`
- `Build & push image`

The golden-path check runs real Docker/App Vault deployment and teardown, container lifecycle,
and restic backup/restore-test coverage. The image check builds both supported Linux
architectures. These are intentionally required even though they take longer than unit tests.

## Other protections

- Every change uses a pull request.
- Unresolved review conversations block merge.
- The rule applies to repository administrators.
- Force-push and deletion of `main` are disabled.
- Required approving reviews remain zero while there is only one eligible maintainer. This avoids
  a permanent self-approval deadlock; the repository's documented two-axis agent review remains
  part of the development process.

## Inspect current state

```bash
gh api repos/niwlekakan/voidtower/branches/main/protection
gh pr checks <pr-number> --required
```

The first command must return a protection document rather than `404 Branch not protected`. The
second command must list the six checks above. A workflow merely running is insufficient; it must
appear in the required set.

## Reconcile drift

From the repository root, reapply the complete desired state atomically:

```bash
gh api \
  --method PUT \
  repos/niwlekakan/voidtower/branches/main/protection \
  --input docs/operations/github-main-protection.json
```

Read the endpoint back after every update. Do not repair one missing context by temporarily
clearing all required checks.

## Rename a workflow job safely

Branch protection identifies a check by its job display name. To rename one:

1. Add or rename the job on a pull request and let the replacement context succeed.
2. Update `github-main-protection.json` with the replacement name while keeping the other five.
3. Apply the complete document and verify the read-back context set.
4. Merge only after `gh pr checks <pr-number> --required` shows the replacement as required and
   successful.

Never remove the old context before the replacement has emitted a successful check run; otherwise
GitHub can leave the pull request permanently waiting for a context that has never existed.

## Emergency recovery

A GitHub service incident or broken required workflow normally means the pull request remains open
until the service or workflow is repaired. Bypassing a red or pending check is not routine recovery.

If repository protection itself prevents the only viable repair:

1. Record the incident, affected pull request, failing context, and reason normal repair is
   impossible.
2. Make the smallest explicit protection edit needed through repository administration.
3. Merge only the reviewed recovery change.
4. Immediately reapply `github-main-protection.json`.
5. Read the protection endpoint back and confirm all six contexts plus administrator enforcement.
6. Run the displaced validation against the merged commit and attach the result to the incident.

Do not delete the protection rule, enable force-pushes, or leave administrator enforcement disabled
as a shortcut.
