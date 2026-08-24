# Proxmox Compatibility Adoption Implementation Plan

Design: `docs/internal/specs/2026-08-25-proxmox-compatibility-adoption-design.md`
Scope: J0-01/J0-03 Proxmox compatibility adoption and page-local durable job tracking

## 1. Add authorization-first target adoption

- Add `backend/src/operations/proxmox_adoption.rs` with typed selectors for the system, configured
  host, guest, storage, and physical-disk targets.
- Add an injectable evidence provider and production read-only Proxmox implementation.
- Authorize the exact registered action before configuration lookup, token decryption, provider
  evidence, resource observation, or capability publication.
- Resolve the seeded system singleton for host create/configure.
- Observe configured hosts under `voidtower.proxmox_host/local/<host-id>`.
- Normalize guests under `proxmox.guest/<host-id>/<node>/<qemu|lxc>:<vmid>`, storage under
  `proxmox.storage/<host-id>/<node>/<storage>`, and disks under
  `proxmox.disk/<host-id>/<node>/<device-path>`.
- Publish only the requested capability and return bounded, redacted compatibility-safe errors.
- Add focused tests for each kind/alias, mismatched or absent evidence, LXC-only restrictions, and
  authorization-before-provider/observation.

## 2. Complete adapter compatibility seams

- Extend the Proxmox adapter's stable legacy selector so host snapshot/test/configure can read the
  existing settings and encrypted legacy secret without exposing the token.
- Apply legacy settings and encrypted replacement tokens only inside `proxmox.host.configure`
  worker execution; remove an old plaintext token setting only after replacement succeeds.
- Retain ordinary multi-host configure behavior for non-legacy IDs.
- Add compatibility-owned candidate-secret cleanup and controlled-upload cleanup as deterministic,
  idempotent follow-up plan steps that never rewrite proven provider success as failure.
- Preserve LXC networking, nesting, on-boot, and immediate-start defaults.
- Persist a bounded external task reference carrying the UPID and safe LXC VMID; parse the UPID for
  reconciliation and include the VMID in the terminal result.
- Add adapter tests for legacy configuration execution/fingerprints, secret cleanup, upload cleanup,
  LXC input secrecy/defaults, task-reference parsing, and VMID-preserving reconciliation.

## 3. Add controlled staging helpers

- Add an encrypted candidate-secret helper used only after canonical authorization. Store opaque
  UUID references and compatibility-owned names; never return or log token material.
- Join multi-host token ID and token secret only in memory before encryption.
- Add a streaming multipart staging helper under `<data_dir>/proxmox-uploads` with generated
  filenames, strict `content`/single-file fields, a 16 GiB bound, safe permissions, and path escape
  rejection.
- Remove staged candidates/files on submission failure; leave accepted artifacts to the adapter's
  durable cleanup step.
- Add tests proving raw credentials and file bytes do not enter job JSON, plans, errors, or audit
  state and that invalid multipart/path inputs create no durable job.

## 4. Convert the multi-host Proxmox compatibility routes

- Add shared `prepare_or_submit` and canonical legacy-plan response helpers to
  `backend/src/api/proxmox.rs`.
- Convert host create/delete, seven guest lifecycle routes, three snapshot routes, disk attach,
  LXC deploy, storage upload/delete, and disk wipe/initialize to the resolver and canonical bridge.
- Preserve optional dry-run request extraction and route-specific input validation.
- Ensure `/shutdown` selects `proxmox.guest.shutdown` rather than delegating to stop.
- Keep informational GETs, monitoring, and VNC ticket issuance synchronous and otherwise unchanged.
- Remove mutation-side `reqwest` calls, `pve_post`, direct host/secret SQL, and optimistic audit
  success records from the adopted handler sections.

## 5. Convert the legacy Proxmox aliases

- Convert `POST /api/vms/proxmox/action` to select one of its six exact guest actions, resolve the
  legacy target with explicit node/kind evidence, and submit a durable job.
- Convert `POST /api/vms/proxmox/config` to stage an optional token reference and submit
  `proxmox.host.configure` against the system singleton.
- Convert `POST /api/vms/proxmox/test` to resolve the stable legacy host and submit the read-result
  `proxmox.host.test` job.
- Keep config/list reads token-redacted and synchronous; keep local libvirt behavior out of scope.
- Update the legacy token regression to prove accepted submission does not apply configuration
  inline and persisted job input contains only a secret reference.

## 6. Strengthen route and bypass tests

- Add an exact registry regression for the 21 Proxmox route mappings and 20 action inventory.
- Add source-inventory tests that isolate multi-host mutation handlers and legacy Proxmox mutation
  handlers while exempting synchronous reads and VNC.
- Require canonical credential, Proxmox resolver, prepare, and submit delegation.
- Forbid provider `.send()`, `pve_post`, mutation SQL, direct secret writes outside controlled
  staging, and optimistic audit-success claims in adopted handler sections.
- Keep the generated authentication/role matrix green and add focused rollback/delete malformed
  input ordering checks where required.

## 7. Type job results and add page-local batch tracking

- Extend `DurableJobSummary` with the safe progress/result/error/timestamp fields already returned
  by the backend.
- Type every Proxmox execution client method as `DurableJobResponse`; retain advisory plan unions.
- Let the existing single-job tracker pass the conclusive job to success callbacks.
- Add a bounded page-local batch tracker for one intentional group of returned jobs. Poll each exact
  ID, tolerate transient errors, show approval and partial terminal states, never auto-approve or
  resubmit, and call refresh only after all jobs are terminal.
- Add compact single/batch notices without creating navigation or approval controls.

## 8. Convert Tower and Void Proxmox interactions

- Update `ProxmoxPage.tsx` host add/delete, lifecycle, snapshots, storage upload/delete, disk
  passthrough/wipe/init, and bulk actions to track accepted jobs and remove synchronous success
  claims.
- Preserve adapter-produced `ChangePlanModal` previews and close modals after accepted submission.
- Track bulk start/stop as the full returned job set and report partial outcomes accurately.
- Update `aios/panels/proxmox.tsx` lifecycle and snapshot actions to the same typed APIs and local
  tracking.
- Continue refreshing the existing synchronous inventory only after conclusive job success or its
  established background polling interval.

## 9. Convert legacy VM and App Vault callers

- Update `VMs.tsx` and `aios/panels/vms.tsx` legacy Proxmox actions to consume and track durable
  jobs while leaving local libvirt calls unchanged.
- Update legacy config/test controls to show the submitted configuration/test job and refresh only
  after success.
- Update `DeployToProxmoxModal.tsx` to generate the bootstrap script locally, submit only safe LXC
  fields, wait for canonical success, read the VMID from the job result, and then show the manual
  bootstrap instructions.
- Surface approval, failure, cancellation, expiration, `needs_attention`, and foreground timeout
  without adding approval decisions or global job history.

## 10. Reconcile roadmap and successor handoff

- Mark all six compatibility domains adopted in `ROADMAP.md` while leaving cross-domain bypass
  closure, shared Jobs/Approvals UX, and durable SSE outstanding.
- Update the Proxmox safety/current-state text to describe durable plans/submissions rather than
  legacy direct execution.
- Create the ignored successor handoff with exact files, verification results, commit state, and
  cross-domain bypass closure as the next bounded slice.

## 11. Verify, review, and commit

- Format only touched Rust files with direct `rustfmt --edition 2021`.
- Run focused resolver, adapter, API, registry, source-inventory, and auth tests.
- Run `cargo test --all-targets --all-features` and
  `cargo clippy --all-targets --all-features -- -D warnings`.
- Run frontend `npm run type-check`, `npm run lint`, and `npm run build`.
- Run schema-migration ownership, repository hygiene, and `git diff --check`.
- Review the complete diff for authorization ordering, credential/upload leakage, provider bypass,
  action mismapping, LXC result continuity, false completion, unrelated churn, and route
  compatibility.
- Remove the tracked internal design/plan from the final public tree while retaining ignored local
  copies, matching the prior adoption workflow.
- Commit one focused Proxmox adoption checkpoint. Do not push.

## Acceptance matrix

| Requirement | Primary evidence |
|---|---|
| Exact 21-route/20-action mapping | Registry table test |
| Authorization before evidence/staging | Resolver denial-order and handler tests |
| Stable canonical target identity | Host/guest/storage/disk resolver tests |
| No handler provider mutation | Cross-file source inventory |
| Advisory plans remain side-effect free | Prepare-only job/approval counts |
| Credentials remain opaque | Secret staging and durable-state redaction tests |
| Uploads remain controlled | Multipart/path/boundary tests and adapter fingerprint tests |
| Restart-safe provider tasks | UPID mock-server and reconciliation tests |
| App Vault LXC remains usable | VMID result and browser-local bootstrap flow |
| Frontends follow canonical state | Strict types, trackers, lint, and build |
| No scope expansion | Diff review and explicit deferred list |
| Full checkpoint health | Backend, frontend, schema, hygiene, and diff gates |
