# Route and Action Registry Convergence Design

## Context

VoidTower currently describes the same security surface in several independent ledgers:

- `api/authz_matrix.rs` assigns a session role to every HTTP route for test coverage.
- `auth/scope_enforce.rs` assigns bearer-token behavior to a selected subset of routes and
  treats absence as denial.
- `voidwatch/risk_class.rs` assigns a risk class to every HTTP route and separately maps
  AI/automation action names to risk classes.
- `voidwatch/denylist.rs` names irreversible operations that always require approval.
- AI ingress modules maintain their own tool/action inventories and read-only distinctions.

Each ledger has a legitimate consumer, but independently editing them permits drift. S0-03
replaces the overlapping route and action classifications with typed metadata that is complete
by construction and exhaustively checked against the live router and action dispatchers.

This is a behavior-preserving security refactor. It must not broaden access, lower a risk class,
remove an approval requirement, or expose a new action to AI. Contradictions discovered during
the migration are made explicit and handled as separately reviewed policy changes.

## Goals

1. Give every registered HTTP route one complete metadata record.
2. Give every non-HTTP action accepted by an AI or automation dispatcher one complete metadata
   record.
3. Derive session, bearer, risk, approval, and AI-exposure decisions from those records.
4. Fail compilation when a record omits a typed field and fail tests when a route or dispatched
   action has no record.
5. Preserve fail-closed behavior for unknown routes and actions.
6. Preserve the named, rationale-bearing irreversibility denylist while preventing it from
   drifting from route metadata.

## Non-goals

- Generating Axum's router from metadata.
- Changing handler response bodies, role checks, token scopes, Voidwatch semantics, or approval
  behavior.
- Resolving the MCP bearer-token contradiction or other authorization mismatches found by the
  migration.
- Adding routes, scopes, AI tools, infrastructure features, or schema changes.
- Moving policy into configuration or making security metadata mutable at runtime.

## Considered approaches

### Typed in-code registries — selected

Add one production Rust module containing a route registry and an action registry. Each record
requires all security dimensions as enum fields. Existing consumers query or project these
records, and exhaustive tests compare them with the live Axum router and action dispatchers.

This approach provides compiler-checked record shape, ordinary Rust review ergonomics, no new
dependency, and a narrow migration path. The router remains hand-written, so route completeness
is enforced by tests rather than compilation.

### Macro-generated router and metadata

A macro could generate both Axum registrations and metadata from one declaration, providing a
stronger compile-time relationship. It would also rewrite the central router, complicate Axum
handler typing, and combine security convergence with a large routing architecture change.
That risk is disproportionate to S0-03.

### External schema with build-time code generation

YAML or JSON could be a language-neutral source of truth. It would add parsing, schema,
generation, and build-script failure modes while weakening direct Rust navigation. VoidTower
has no current cross-language consumer that justifies those costs.

## Registry model

The new shared module owns stable key and policy types. Exact Rust names may be adjusted during
test-driven implementation, but the semantic fields are fixed by this design.

```rust
struct RouteMetadata {
    method: HttpMethod,
    path: &'static str,
    session: SessionPolicy,
    bearer: BearerPolicy,
    credential: CredentialPolicy,
    risk: RiskClass,
    approval: ApprovalPolicy,
    ai_exposure: AiExposure,
}

struct ActionMetadata {
    name: &'static str,
    ingress: ActionIngress,
    risk: RiskClass,
    approval: ApprovalPolicy,
    ai_exposure: AiExposure,
}
```

`SessionPolicy` distinguishes public routes, minimum session tiers, and routes whose handler
uses another credential. `CredentialPolicy` identifies that non-session mechanism rather than
collapsing bearer API tokens, webhook HMAC, node tokens, and pairing codes into one category.
`BearerPolicy` explicitly says public, unscoped token-to-session access, required scope, or denied;
no registered route relies on absence from a table to communicate intent. Unscoped access is
limited to the two embed-router routes that intentionally bypass scope enforcement today.

`RiskClass` remains Voidwatch's existing ordered classification. `ApprovalPolicy` records
whether approval is inapplicable to a read-only action, follows the risk/mode ladder, or is always
required by the hard denylist. It cannot override a mutating irreversible action to become less
restrictive. `AiExposure` explicitly distinguishes actions callable by an AI ingress, actions
visible only as context/read data, and actions unavailable to AI.

The HTTP key is the exact method and Axum matched-path pattern already used by middleware. The
action key is the canonical string passed to `voidwatch::evaluate`, not a display label.

## Ownership and consumers

The shared metadata module is the only owner of overlapping classifications:

- Session-matrix tests project `SessionPolicy` and continue probing real handlers.
- Bearer middleware resolves `BearerPolicy` by the request's method and `MatchedPath`.
- Route risk lookup resolves `RiskClass` from the same route record.
- Voidwatch action lookup resolves risk and approval from `ActionMetadata`.
- MCP and integration tests validate that every dispatched tool/action is registered with the
  matching AI exposure and ingress kind.
- The named irreversibility denylist retains item IDs, descriptions, and rationale, but its
  route references must resolve to registry records whose risk and approval are both
  irreversible/always-required.

Compatibility projection constants may exist temporarily during migration, but they must be
generated from or validated against the shared records and removed before S0-03 completes.

## Completeness and invariants

The implementation enforces these invariants:

1. Every `(method, path)` mounted by `api::router()` appears exactly once in the route registry.
2. No route metadata record refers to an unmounted route.
3. Route keys and action names are unique.
4. Every record explicitly supplies every security dimension.
5. Every action accepted by MCP, Studio, webhook, or automation dispatch appears exactly once in
   the action registry.
6. Unknown bearer routes remain denied, and unknown AI/automation actions retain Voidwatch's
   safest risk behavior.
7. A hard-denylisted route or action is irreversible and always requires approval.
8. AI-callable mutating actions have a non-read risk class and pass through the existing
   Voidwatch evaluation choke point.
9. Public session access and public bearer access are independent, explicit decisions.
10. Handler-managed credential routes identify the concrete credential kind and cannot
    accidentally inherit public or scoped bearer behavior.

The existing source parser for Axum route declarations is consolidated into one test helper.
Parser failure is itself a test failure; it cannot silently produce an empty or partial route
set.

## MCP credential contradiction

`GET /api/mcp` and `POST /api/mcp/message` are currently described by the session matrix as
non-session-credential routes and their handlers expect bearer API tokens. The global bearer
scope middleware has no entries for them and therefore appears to deny token-originated requests
before the handler validates its token. S0-03 records the shipped middleware outcome as denied
while retaining the concrete handler credential classification and adds a regression that makes
the mismatch visible.

Enabling these routes for bearer credentials changes the externally reachable security surface.
That correction requires its own route-level tests and explicit policy authorization after the
registry migration; it is not hidden inside this structural task.

## Migration sequence

1. Add failing invariant tests for duplicate/missing route metadata and missing action metadata.
2. Introduce typed metadata and mechanically transcribe the existing route classifications.
3. Point session-matrix tests at registry projections and remove their independent role ledger.
4. Point bearer middleware at explicit registry metadata while proving response equivalence for
   scoped, public, denied, and human-session requests.
5. Point route and action risk lookup at the registry without altering classifications.
6. Validate the named denylist against registry risk and approval fields.
7. Validate AI/MCP/automation dispatcher inventories against action metadata.
8. Delete superseded tables, duplicate parsers, and compatibility projections.
9. Run real-router authorization probes, Voidwatch tests, golden workflows, and repository gates.

The migration should be split into reviewable commits if needed, but the branch must not land in
a half-converged state where two editable sources remain authoritative.

## Test strategy

Tests begin red against the current independent ledgers. The acceptance suite includes:

- bidirectional live-router-to-registry equality;
- duplicate route and duplicate action rejection;
- complete-field construction enforced by Rust types;
- session-role projection equivalence and representative real-router probes;
- bearer-policy equivalence for public, correctly scoped, incorrectly scoped, session-only, and
  unregistered requests;
- risk-class equivalence for all existing routes and canonical actions;
- hard-denylist invariants for irreversible risk and mandatory approval;
- dispatcher-to-action-registry equality for MCP and other structured action ingresses;
- unknown route/action fail-closed regressions;
- explicit MCP bearer contradiction regression with no access broadening.

## Documentation and observability

`docs/codebase-map.md` and `docs/api.md` will identify the registry as the authoritative security
inventory and explain that handler checks remain defense in depth. Test failures name the missing
or conflicting method/path/action and policy dimension so future contributors can correct the
registry without reverse-engineering the invariant.

No runtime logging, API schema, telemetry, or UI behavior changes are required for convergence.

## Authorization boundary

The implementation necessarily touches `backend/src/auth/` and Voidwatch risk/denylist files,
which are forbidden zones under `CLAUDE.md`. A narrowly scoped accepted ADR must grant only the
shared registry and named consumer files. It must explicitly forbid changes to role semantics,
scope definitions, mode-ladder behavior, approval outcomes, secrets, database schema, and CI.
