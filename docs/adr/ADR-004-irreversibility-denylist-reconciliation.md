# ADR-004 — Irreversibility denylist: reconcile EDD §10.3 vs gap-analysis P0.4

**Status:** Accepted (signed by operator VoidTower DevTeam on 2026-07-09)
**Date:** 2026-07-09
**Expires:** at P0 phase exit (`adr.sh revoke ADR-004`)

## Context

`docs/gap-analysis.md` §2 (P0.4) and `docs/edd.md` §10.3 both describe a hardcoded,
API-immutable "irreversibility denylist" — actions that require human approval regardless of
Voidwatch mode, including YOLO. `.devteam/active/P0-04-irreversibility-denylist.md` is the task
spec implementing this, and it self-reports `BLOCKED` because the two source documents name
**different, mostly non-overlapping** lists:

| EDD §10.3 | gap-analysis §2 P0.4 |
|---|---|
| host power actions on the control-plane's own host | — |
| deletion of the last snapshot/backup of a resource | — |
| `keep_data=false` app removal | — |
| device decommission | — |
| Voidwatch policy/mode changes | policy/mode edits |
| secrets master-key operations | secrets export |
| agent update rollouts | self-update trigger |
| — | disaster-recovery import/reset |
| — | `docker.sock`-level raw operations |
| — | disk wipe/format |
| — | firewall disable |

Only "policy/mode changes" is a clean match; "secrets master-key operations" (EDD) and "secrets
export" (gap-analysis) are related but distinct (see Decision). The remaining eight items each
appear in only one list. `docs/gap-analysis.md`'s framing note (top of file) says the plan
"supersedes... retargeted per this plan" for the sections it lists, but §10.3 is not among the
sections gap-analysis claims to supersede (gap-analysis supersedes EDD §2, §6, §16 only; §3.2/§10
are named as "remain in force as target specifications, retargeted per this plan" — i.e. gap
analysis's P0.4 wording is a retargeting, not a replacement). Silently picking one list is
exactly the ambiguity CLAUDE.md says to escalate rather than guess through, and the task spec
says so explicitly. This ADR makes that call.

I (the P0-04 worker) also verified each candidate action against source, since the task spec's
endpoint-mapping table was marked draft/unconfirmed:

- **self-update trigger**: `POST /api/system/update` (`backend/src/api/system.rs`) and
  `POST /api/updates/{voidtower,odysseus,docker,os}/apply` (`backend/src/api/updates.rs`) —
  confirmed both exist and both mutate the running system's binaries/images.
- **disaster-recovery import/reset**: `backend/src/api/disaster.rs` has `import_config` (line
  231), `emergency_reset_admin` (line 361), and also `emergency_disable` (line 417, **not** in
  either source list — disables auth/lockdown protections; candidate addition, see Decision) and
  `export_config` (line 90, read-only-ish but bundles secrets — see next item).
- **secrets export**: `GET /api/secrets/:id/reveal` (`secrets.rs:122`) is single-secret
  plaintext reveal. `disaster.rs`'s `export_config` (line 90) is a *bulk* config export;
  confirmed by reading it — it is gated separately (`require_owner`, disaster.rs:23) but its
  output composition wasn't re-verified line-by-line for secret-material inclusion in this ADR
  pass. Flagging for the implementer: if `export_config` embeds decrypted secret values, it
  belongs on this list too; if it only exports references/ciphertext, it doesn't. **Verify in
  source before implementing, not assumed here.**
- **policy/mode edits**: `POST/PATCH/DELETE /api/policy/rules*` (`backend/src/api/policy.rs`)
  confirmed in the route map. The mode-setting endpoint is P0-03's deliverable and doesn't exist
  yet — P0-04 depends on P0-03 per the task spec's own "Depends on" line, so this row can't be
  wired until P0-03 lands regardless of this ADR.
- **`docker.sock`-level raw operations**: confirmed via grep — `bollard::Docker::connect_with_unix_defaults()`
  is called from `backend/src/containers/mod.rs:99` (the shared helper) and directly inline in
  `backend/src/api/containers.rs:205` (log streaming). All container mutation (start/stop/
  restart/exec) already routes through typed verbs in `containers/mod.rs`, not raw socket calls
  from handlers — so there is no *additional* raw-socket-bypassing-typed-verbs endpoint to gate
  beyond what P0-01/P0-02's choke-point work already covers. This denylist item is best read as
  "no action verb may skip Voidwatch by dropping to the raw Docker API," which is an invariant
  the P0-01 choke point + P0-04's `denylist_has_no_api_mutation_path`-style test can assert,
  **not** a specific extra endpoint to add to the constant list.
- **disk wipe/format**: `POST /api/storage/format` (`backend/src/api/storage.rs`) confirmed in
  route map. `proxmox.rs` also exposes node disk wipe/init per the route map ("node disk list/
  SMART/wipe/init") — same semantic action on a different resource type, should be covered too.
- **firewall disable**: `POST /api/firewall/action` with body `{"action": "disable"}` confirmed
  at `backend/src/api/firewall.rs:281,284,289` — `FirewallActionRequest.action` is a string enum
  `"enable"|"disable"|"reload"|"reset"`; only `"disable"` (and arguably `"reset"`, unverified
  semantics) should hit the denylist, not the whole endpoint.

## Decision

Take the **union** of both lists, not gap-analysis alone. Reasoning: the denylist's entire
purpose is "always require a human," and the cost of a false positive (one extra approval click)
is asymmetric with the cost of a false negative (an unattended irreversible action). Given that
asymmetry, and given gap-analysis's own framing that EDD §10.3 "remains in force... retargeted"
rather than superseded, narrowing to gap-analysis's seven items and dropping EDD's four
control-plane/backup/decommission/master-key items would be a silent capability *reduction* an
ADR should not make unilaterally. Union is the conservative reading; the operator can strike
items in review if any are considered out of scope for P0.

**Reconciled hardcoded denylist for P0-04** (dedup'd, EDD and gap-analysis items merged, my
`emergency_disable` finding added as a candidate for operator sign-off):

1. Self-update / agent-update trigger — `POST /api/system/update`,
   `POST /api/updates/*/apply` (all four targets)
2. Disaster-recovery import/reset — `POST /api/disaster/import-config`,
   `/emergency-reset-admin`, and **candidate addition** `/emergency-disable` (operator: confirm
   in review — it wasn't in either source list but disables protections, same risk class)
3. Secrets export/reveal — `GET /api/secrets/:id/reveal`; `POST /api/disaster/export-config`
   **iff** it includes decrypted secret material (implementer must verify in source first)
4. Secrets master-key operations (EDD-only item) — no dedicated endpoint found in the route map
   in this pass; implementer must locate master-key rotation/access code (likely
   `backend/src/secrets/` or inline in `secrets.rs`) before implementing this row, or escalate
   again if no such endpoint exists yet
5. Policy/mode edits — `POST/PATCH/DELETE /api/policy/rules*`; P0-03's mode-setting endpoint
   (blocked on P0-03 landing)
6. No-bypass invariant for `docker.sock` raw operations — assert via test that all mutating
   container actions route through the typed verb layer, not a direct policy-list endpoint
7. Disk/storage wipe or format — `POST /api/storage/format`; Proxmox node disk wipe/init
   endpoints in `proxmox.rs` (implementer: get exact route names from that file)
8. Firewall disable — `POST /api/firewall/action` where `action == "disable"` (and confirm
   `"reset"` semantics before deciding whether it belongs here too)
9. Host power actions on the control-plane's own host (EDD-only) — implementer must confirm
   whether such an endpoint exists (distinct from generic device power actions on *other*
   hosts, which are not on this list)
10. Deletion of the last remaining snapshot/backup of a resource (EDD-only) — count-dependent
    condition, not a fixed endpoint; implementer must add a check in the backup-deletion path
    (`backend/src/api/backups.rs`), not just gate the endpoint unconditionally
11. `keep_data=false` app removal (EDD-only) — `apps.rs`'s delete route with that flag set
12. Device decommission (EDD-only) — implementer must confirm which endpoint represents
    "decommission" vs. ordinary resource deletion

Items 4, 9, 10, 12 could not be fully mapped to source in this ADR pass; the implementer should
verify each before writing its acceptance test, and escalate again (narrowly, per-item) if an
item has no corresponding endpoint yet to gate.

## Explicitly NOT granted

- This ADR does not grant any *new* forbidden-zone path beyond what ADR-001 already grants.
  `backend/src/policy.rs` is already in ADR-001's `granted-paths` block; this ADR exists to
  resolve the P0.4 *content* question, not to re-open path access.
- Does not authorize touching `backend/src/db/mod.rs` — the denylist is a compiled-in constant
  per the task's explicit constraint, not a table, so no schema grant is needed or given here.
- Does not authorize adding condition-based checks (item 10, "last backup") anywhere outside
  the specific mutating handler being gated — no general backup-lifecycle refactor.

## Constraints

1. The denylist itself remains a compile-time constant in `backend/src/policy.rs` (or a new
   `backend/src/voidwatch/` submodule under ADR-001's existing grant) — no DB table, no
   `PATCH`/`DELETE` route, per the task spec's explicit requirement and ADR-001 Constraint 2.
2. Applies to **every** actor class, not just `ai`/`automation` — EDD says "these always require
   human approval," not "AI approval." A `user`-actor operator hitting these endpoints in YOLO
   mode must also be blocked. (Confirms the task spec's own
   `denylist_applies_regardless_of_actor_class` acceptance test is correctly scoped.)
3. Items without a confirmed source mapping (4, 9, 10, 12 above) must not be silently dropped
   nor silently invented — implementer verifies against source per item, and narrowly escalates
   any single item that has no corresponding endpoint to gate yet, rather than blocking the
   whole task again.
4. `denylist_has_no_api_mutation_path` must assert *structurally* (e.g. grep/`include_str!`
   over route registration) that no route can alter the constant, not just that today's routes
   happen not to.

## Consequences

Once accepted, `.devteam/active/P0-04-irreversibility-denylist.md`'s `**ADR:**` field should be
updated to `ADR-004` and its `## Status:` line flipped off `BLOCKED`, unblocking implementation
against the reconciled 12-item list above. `gates.sh` will still enforce that any
forbidden-zone path in the diff falls within ADR-001's `granted-paths` (this ADR adds no new
paths). This ADR expires with the rest of the P0 grant family at phase exit.
