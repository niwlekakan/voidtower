# Household membership and resource grants

VoidTower household authorization is additive to the existing local/Authentik/OIDC user identity. A household does not create a second user identity, and a grant never creates a second resource identity: `resources.id` remains the canonical operational and CMDB resource UUID.

## Create and manage a household

An owner or administrator session can create a household:

```http
POST /api/households
Content-Type: application/json

{"name":"Home"}
```

The creator becomes the household owner. `name` is limited to 128 bytes and cannot contain control characters. `GET /api/households` returns the caller's membership rows. Household member and grant administration is limited to a global owner/administrator or an active household owner/administrator.

The three JSON mutation routes in this section authenticate the session and household role before reading or parsing the request body. Bodies are capped at 64 KiB. Unauthenticated requests therefore return `401` even when the JSON is malformed or oversized; authenticated malformed JSON returns `422`, an unsupported content type returns `415`, and an oversized body returns `413`.

```http
POST /api/households/{household_id}/members
Content-Type: application/json

{"user_id":"<existing-user-id>","role":"member"}
```

Supported membership roles are `owner`, `admin`, `member`, `guest`, and `device`. An optional `expires_at` Unix timestamp must be in the future. Removing a member is a destructive authorization operation; the last active household owner cannot be removed, and an administrator cannot remove their current membership through this endpoint.

## Resource grants

A grant binds an existing household member to the canonical resource UUID and one permission:

- `read`: resource reads and capability reads;
- `operate`: the `read` permission plus canonical durable action authorization after the action's existing role and bearer-scope checks;
- `admin`: the highest household grant, assignable only by a global owner.

```http
POST /api/households/{household_id}/grants
Content-Type: application/json

{"user_id":"<member-id>","resource_id":"<resources.id>","permission":"read"}
```

The target user must be an active member and the resource must already exist. Optional `expires_at` timestamps are bounded to future values. Grant updates are idempotent for the household/resource/user tuple and clear a prior grant revocation.

Low-trust `member`, `guest`, and `device` users receive only explicitly granted resource reads, plus the one canonical action listed under [grant-scoped canonical action](#grant-scoped-canonical-action). Cross-household resource access returns the bounded `resource_grant_required` error and does not disclose whether an ungranted resource exists. Owner, administrator, and operator roles retain their existing operational read and mutation boundary.

## Grant-scoped canonical action

A grant is additive within the existing identity model; it also authorizes exactly one canonical mutation class and nothing more. The allowlist is application code, not configuration: `GRANT_SCOPED_ACTIONS` in `backend/src/action_registry.rs` currently names only `automation.run`.

- The action registry still declares this action's operator role. A human session whose role is `viewer`, `member`, `guest`, or `demo` may reach the canonical plan/submit boundary only because the action is on the allowlist. Every other durable mutation keeps its declared operator/admin role and is refused with `forbidden` before planning, even when a matching grant exists.
- The canonical boundary re-checks the household grant (`permission >= operate`, active membership, no expiry, no revocation) inside `prepare` and `submit` before capability resolution, planning, idempotency acceptance, or any provider access. A missing, read-only, expired, or revoked grant fails closed with `resource_grant_required`.
- The allowlist never reaches a compatibility route or a machine ingress: `POST /api/automation/:id/run` keeps its operator session boundary, and bearer, MCP, studio, webhook, scheduler, and local-CLI credentials are unchanged. There is no path where a grant substitutes for an API-token scope or AI exposure.
- Policy, the action's approval class, the durable job, idempotency, audit, and durable events are unchanged. The job records the member as its human actor and the `http_session` ingress. A grant decision is never an approval: risk-ladder or `always` approval still applies.
- A new grant-scoped action is a reviewed registry edit, and `grant_scoped_actions_are_an_explicit_bounded_allowlist` fails if the allowlist and the filtered registry disagree.
- Revoking a grant reconciles work that was already accepted: the subject's non-terminal jobs for that resource and that grant-scoped action become `cancelled` with `household_grant_revoked` in the same transaction, so a queued or approval-pending request cannot be approved and executed after its authority is gone. Jobs an operator submitted on the same resource are untouched because they never depended on the grant.
- A member can read the grants that apply to itself through `GET /api/households/{household_id}/grants/me`; the manager-only grant table stays unreadable to it. The web surface uses that read to decide whether the action control is offered at all.
- Grant issuance is a delegation: an active household owner/administrator may grant any existing `resources.id`, including a resource their own account cannot read, because the grant (not the granter's visibility) is the authority. That power is deliberate and is why grant changes are audited.
- Cancellation keeps the plain role rule, so a grant-scoped member cannot cancel its own job through `POST /api/jobs/{id}/cancel`; an owner or administrator cancels or resolves it. This asymmetry is intentional: cancellation is a broader authority than the one grant-scoped action.

## Household web surface

The supported web client exposes `/households` to every authenticated role. A global owner/administrator, or an active household owner/administrator, sees the household selector, member administration, the resource-grant editor and revoker, and the household consent list. Every other member sees the households they belong to, the canonical resources their own grants make visible (through the grant-filtered `GET /api/resources`), a plan-then-submit control for a granted `automation.run` resource whose capability is `available`, and the consent recorder for their own local-only consent.

The page renders explicit loading, empty, and failure states; a partial read is reported as a failure with a retry rather than as an empty projection. It never offers a control the server would refuse: the run control appears only for an automation resource whose reported capability is `available`, and a refusal is rendered with its bounded error code instead of a generic failure.

## Revocation and recovery

`DELETE /api/households/{household_id}/members/{user_id}` marks the membership revoked, removes that user's compatibility sessions and API tokens, and transitions their nonterminal canonical jobs to `cancelled` with the bounded `household_membership_revoked` error code. A bearer-token cache hit revalidates the token row, so deleting a token cannot leave a cached compatibility session usable for the cache lifetime. Existing terminal jobs remain unchanged.

Revocation and grant decisions are enforced before resource reads or canonical provider planning. Secrets, raw tokens, SQL diagnostics, and provider output are not returned in household error envelopes.

Household administration writes audit rows for `household.create`, `household.member.add`, `household.member.revoke`, `household.grant`, and `household.grant.revoke`. Each row records the authorized actor, the canonical household or `resources.id` that changed, and only the member id plus the role or permission token as bounded detail; names, credentials, and provider material never reach the audit table. Like every other audit writer in the product the append happens after the mutation and an insert failure is logged rather than rolling the mutation back, so the audit trail is best-effort rather than transactional.

## Consent, purpose, and privacy operations

Household privacy operations use the existing session identity and household/resource-grant boundary. They do not create a second identity or resource identifier. An active household member can grant or withdraw their own consent; an active household owner/administrator can administer consent for another active member. A resource-scoped consent also requires a `read` grant for the subject.

```http
POST /api/households/{household_id}/consents
Content-Type: application/json

{"user_id":"<member-id>","purpose":"privacy_export","domain":"inventory","resource_id":"<resources.id>"}
```

Consent `purpose` values are `privacy_export`, `ai_assistance`, `cloud_export`, and `support`. Supported sensitive domains are `identity`, `inventory`, `telemetry`, and `configuration`. `destination` defaults to `local`; `destination:"cloud"` is rejected unless `ai_cloud_export:true` is explicitly present. Optional `expires_at` and `retention_until` timestamps are bounded, and withdrawal is recorded rather than deleting the consent record. `GET /api/households/{household_id}/consents` returns a versioned page envelope for a household manager. The default and maximum `limit` is 200; pass the decimal `cursor` returned as `next_cursor` to continue. `has_more` is true whenever another page exists, and an exhausted page returns `next_cursor:null`. Invalid, negative, or oversized `limit`/`cursor` values return bounded `400` errors rather than silently truncating; `limit=0` is invalid, while `cursor=0` is the valid first-page default, and a cursor too close to the upper bound for the requested page size is rejected so `next_cursor` remains representable. Authentication and household-manager authorization are evaluated before consent query validation, so unauthenticated malformed requests remain `401`. Cursors are bounded decimal offsets rather than snapshot tokens, so concurrent consent changes can shift later page boundaries. `DELETE /api/households/{household_id}/consents/{consent_id}` withdraws an active record and returns a versioned withdrawal envelope.

Export and deletion requests are durable, idempotent privacy-operation records. They are claimed by the local privacy executor, which uses bounded leases and retry/recovery state. Local exports are written as owner-only JSON artifacts capped at 256 KiB and 200 rows; the artifact endpoint verifies the stored SHA-256 before serving. Worker startup removes only UUID-named partial artifacts that have no durable row; final artifact files are never deleted solely because a concurrent database row is temporarily absent, and a formal expiry/cleanup policy remains future work. Cloud export remains unavailable even when a request is persisted. Deletion is local-only: inventory grants and selected configuration rows are removed only after legal-hold and retention checks; request tombstones, audit history, canonical resources, and users are retained. A queued request can be cancelled immediately; a processing request records `cancel_requested` for the owning worker, and failed/cancelled requests can be retried. A broad deletion is blocked by any matching hold or future retention record, including records scoped to an individual resource. Resource-scoped deletion is currently supported only for `inventory`; it removes that resource's grant, matching consent, matching export artifacts, and any unscoped inventory artifact for the same subject, while preserving other-resource state. Privacy worker execution is serialized within the runtime; committed partial artifacts are repaired on recovery before new work is claimed. Identity and configuration deletion are subject-wide and must omit `resource_id`; the local executor fails closed with `multi_household_scope_requires_review` when the subject has more than one active household membership, so no global identity/configuration rows are deleted across an unchecked household boundary.

```http
POST /api/households/{household_id}/privacy-requests
Content-Type: application/json

{"kind":"export","user_id":"<member-id>","purpose":"privacy_export","domain":"inventory","resource_id":"<resources.id>","minimization":"metadata","idempotency_key":"export-2026-01"}
```

`kind` is `export` or `deletion`, `purpose` must match the kind, and `minimization` is `metadata` or `records`. `idempotency_key` must be 1–128 ASCII characters, begin with an ASCII letter or digit, and then contain only ASCII letters, digits, `.`, `_`, `:`, or `-`; explicit JSON `null` is not accepted for `destination`. JSON intake requires `Content-Type: application/json` and is bounded to 64 KiB. Export requires an active matching consent and a resource grant when scoped; an expired `expires_at` or `retention_until` consent is not usable. Cloud export remains denied unless the matching consent explicitly opted into the cloud destination. Deletion is local-only, remains auditable, and returns a durable `queued` status. A successful deletion also invalidates and removes local export artifacts for the selected scope; request tombstones, audit history, canonical resources, and users remain retained. Repeating the same household/subject/kind/idempotency key with the same intent returns the original request; changing purpose, domain, resource scope, destination, or minimization returns a bounded `409 conflict`. Resource-scoped deletion requests for `identity`, `configuration`, or `telemetry` are rejected with a bounded `400`; those domains use subject-wide deletion. `POST /api/households/{household_id}/privacy-requests` returns `202` with the versioned request envelope for both new and idempotent replayed requests. `GET /api/households/{household_id}/privacy-requests/{request_id}` exposes the same versioned envelope with bounded state and a generic operator-review message for failed requests; raw diagnostics are not returned. Cancel and retry return a versioned mutation envelope with explicit `cancelled`, `cancel_requested`, and `queued` booleans. Request status and artifact download both require active subject membership plus actor/manager authorization. `GET /api/households/{household_id}/privacy-requests/{request_id}/artifact` serves only a succeeded, digest-verified local artifact to an authorized subject or household manager and reads at most the bounded artifact size.

Lifecycle changes append redacted audit records and versioned durable events with a correlation ID. Audit rows retain the authorized actor linkage; durable privacy events omit actor IDs, household/resource identifiers, and other identity material because events are bounded history/invalidation signals rather than an authorization source. Privacy records store only declared subject, purpose, domain, resource scope, destination, minimization, and bounded status fields; secrets, raw tokens, and provider diagnostics are excluded. The local executor is integrated into the operation runtime with cancellation-aware shutdown. Current evidence is integration-verified for isolated Axum/SQLite request/response behavior and unit-verified for the local executor's export, legal-hold, retention, cancellation, lease recovery, bounded artifact, stale-grant filtering, malformed-row rejection, multi-household fail-closed deletion, and deletion paths. Supported-host/browser/runtime/release qualification, external cloud delivery, legal retention-worker policy beyond the explicit database hold check, and household UI remain later gates.

Privacy requests are a specialized local data-rights job boundary: they persist complete intent, idempotency, leases, cancellation, audit, redacted events, and bounded retry state, and the executor can mutate only its allowlisted local privacy tables. It never invokes a provider or canonical resource mutation adapter. The generic resource action/job contract remains the required boundary for provider/resource mutations; extending privacy operations into that contract is a separate design decision, not implied by these routes.

## Current support boundary

The household membership/resource-grant and privacy-contract seams are `integration-verified` by real Axum-router tests against SQLite, including the grant-scoped canonical action matrix and the administration audit rows. The `/households` web surface is `runtime-verified` for one bounded workflow on a built server with the production frontend bundle: owner bootstrap, household and membership creation, an `operate` grant on a canonical resource, a member's plan/submit of `automation.run` producing a durable job that executed successfully, a member-recorded local consent, grant revocation, the member's resulting `403 resource_grant_required`, and the audit trail for every step (the H5-03 continuity handoff and `docs/internal/evidence/h5-03/`).

Everything else about the household domain stays experimental and outside 1.0 claims: external cloud export delivery, retention policy beyond the explicit database check, other sensitive domains, cross-household UI administration of households the caller is not a member of, and install/upgrade/release qualification of this surface.
