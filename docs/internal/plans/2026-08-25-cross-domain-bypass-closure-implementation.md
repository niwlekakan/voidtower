# Cross-Domain Durable-Operation Bypass Closure Implementation Plan

Design: `docs/internal/specs/2026-08-25-cross-domain-bypass-closure-design.md`
Scope: J0-01/J0-03 final six-domain bypass closure and public asynchronous mutation contract

## 1. Extend canonical invocation to authenticated webhooks

- Add `InvocationContext::Webhook { source_id }` in `backend/src/operations/invocation.rs`.
- Map it to webhook ingress, an automation actor/policy identity, stable source label, and bounded
  idempotency scope without assigning a human role or bearer scopes.
- Require webhook durable actions to be explicitly webhook-enabled and AI-callable.
- Extend exhaustive authorization, actor, ingress, policy, and idempotency tests for the new variant.
- Change the three durable container lifecycle action declarations to accept both HTTP and webhook
  ingress without changing their session, bearer, risk, approval, retry, or recovery metadata.

## 2. Reuse authorization-first container resolution

- Extract the current compatibility route's Docker availability, read-only list, exact/name/short-ID
  normalization, resource observation, and single-capability publication into a focused internal
  helper in `backend/src/api/containers.rs`.
- Keep authorization before Docker evidence and resource/capability writes.
- Use the helper from the existing session/bearer route with no request/response change.
- Add focused tests for webhook denial order, selector normalization, and exact capability
  publication.

## 3. Adopt the Odysseus webhook container branch

- Change the structured-action mapping in `backend/src/api/integrations.rs` so container actions
  select `container.start`, `container.stop`, or `container.restart`; service actions keep their
  existing direct `start`, `stop`, or `restart` metadata.
- After webhook-secret authentication, route container dry runs through canonical prepare and
  execution through canonical submit with the incoming/generated idempotency key.
- Return the canonical plan response or versioned `202` job envelope from the container branch.
- Remove duplicate manual Voidwatch evaluation, direct `containers::container_action`, and
  handler-side success audit for that branch. Preserve automation and service behavior.
- Map `/api/integrations/webhooks` to the three durable container actions in route metadata and add
  the route to the adopted inventory with handler-managed webhook-policy validation.
- Update action-registry tests to understand the mixed durable/direct structured action route.

## 4. Adopt App Vault proxy exposure and remove implicit open-UI mutation

- Extract a narrow internal create-plan/submit helper in `backend/src/api/proxy.rs`; use it from the
  standard proxy create handler.
- Convert `backend/src/api/apps.rs::expose_app` to build the established proxy `CreateRequest`, use
  the authenticated session credential, and return its canonical durable response.
- Remove `create_proxy_record` and its handler-side database/nginx/audit-success path.
- Map `POST /api/apps/:project_name/expose` to `proxy.rule.create` and add it to the adopted route
  inventory.
- Rewrite `open_ui` as a read-only lookup of an already-valid embed proxy. Remove proxy row writes,
  nginx file/reload calls, firewall changes, and detached mutation work.
- Reclassify the open-UI route as read risk and add regression tests proving no state changes.
- Type App Vault exposure as a durable response in the frontend API client if a client method is
  present or added; keep open-UI's response shape compatible.

## 5. Add the central executable bypass inventory

- Add focused inventory tests under `backend/src/operations/registry.rs` or a dedicated test-only
  operations module.
- Assert the exact 48 adopted route keys, action mappings, and security-policy dominance.
- Inspect each adopted handler/helper region for canonical prepare/submit delegation and reject
  direct provider mutation, authoritative database writes, detached execution, and success audit.
- Scan direct six-domain provider symbols across API, integration, main/CLI, and adapter sources.
- Permit adapter-only calls and an exact count-bounded exception ledger for App Vault/model Compose,
  AI proxy settings, service/automation webhook execution, and Proxmox VNC.
- Fail on new files, new callsites, missing exceptions, increased counts, or obsolete helper names.

## 6. Make main six-domain pages follow submitted jobs

- Reuse `useDurableJobTracker` and `DurableJobNotice` in Containers, Container Detail, Firewall,
  Proxies, Backups, and Dashboard.
- Register every accepted lifecycle, Compose, firewall, proxy/nginx, backup, and restore-test job.
- Refresh domain reads only from conclusive-success callbacks; do not refresh immediately after
  acceptance.
- Preserve advisory plan modals while changing confirmation copy/state to submission semantics.
- Ensure approval, failure, rejection, cancellation, expiry, `needs_attention`, and polling timeout
  remain visible without replay.

## 7. Make native panels follow submitted jobs

- Convert native Containers, Firewall, Proxies, and Backups panels from raw mutation `fetch` calls
  to the typed API client or a shared checked request returning `DurableJobResponse`.
- Correct stale firewall request fields and paths while preserving UI meaning.
- Register accepted jobs in the page-local tracker, expose a durable notice, and reload only after
  conclusive success.
- Keep informational reads and logs direct.
- Extend the central frontend source inventory so each six-domain mutation caller file must contain
  durable tracking and cannot discard the mutation response.

## 8. Publish the asynchronous API contract

- Add a Durable Operations section near the top of `docs/api.md` with canonical resource, plan,
  submit, job, cancel, approval, and event-history endpoints.
- Document idempotency, `202`, job states, immutable plan/approval binding, retry/recovery,
  cancellation, errors, redaction, and compatibility dry-run behavior.
- Publish the grouped 48-route six-domain adoption boundary without exposing internal design files.
- Document the mixed webhook response, durable App Vault exposure, read-only open-UI lookup,
  ephemeral VNC, and deferred direct-domain exceptions truthfully.
- Update `docs/integrations/odysseus.md`, `README.md` where appropriate, and `ROADMAP.md`.
- Mark bypass closure/public contract complete while leaving shared Jobs/Approvals UX and durable SSE
  incomplete and next in execution order.

## 9. Focused regression testing

- Add invocation unit tests for webhook authorization, actor/ingress identity, policy, idempotency,
  and session/bearer non-regression.
- Add real-router webhook tests for invalid secret, dry-run no-job behavior, accepted container job,
  policy denial, and unchanged service/automation behavior.
- Add App Vault exposure tests proving no proxy mutation occurs before worker execution.
- Add open-UI tests proving existing-proxy projection and no database/provider writes.
- Run registry, source-inventory, operation-adoption, container, proxy, integrations, auth matrix,
  and scope-bypass focused tests while iterating.

## 10. Final verification and handoff

- Run `cargo test --all-targets --all-features` and
  `cargo clippy --all-targets --all-features -- -D warnings`.
- Run frontend type-check, lint, and production build.
- Run schema migration ownership, repository hygiene, tracked secret scan if available, and
  `git diff --check`.
- Run the real Docker/App Vault/restic golden path if the local runner exposes its required daemon
  and environment; otherwise record the exact environmental limitation without weakening tests.
- Commit the implementation locally without pushing.
- Remove the design and plan from the tracked final tree while preserving their ignored local
  copies, then write an ignored handoff with exact verification and the next bounded slice.

## Acceptance matrix

| Requirement | Evidence |
|---|---|
| Webhook container calls cannot execute Docker directly | Typed webhook context, durable route test, source inventory |
| Webhook denial precedes evidence/observation | Resolver denial-order test |
| App exposure cannot write nginx directly | Durable job test and removed helper inventory |
| App URL lookup is side-effect free | Database/provider no-mutation test and source inventory |
| Exactly 48 compatibility routes are adopted | Registry equality regression |
| Deferred legacy paths cannot grow silently | Exact count-bounded exception ledger |
| UI does not treat `202` as provider success | Tracker callbacks and frontend source inventory |
| Public contract matches implementation | API documentation plus route/job registry tests |
| Shared Jobs/Approvals and durable SSE remain deferred | Roadmap and documentation scope assertions |
