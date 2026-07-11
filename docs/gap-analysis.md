# VoidTower — Repo Reality vs. EDD v0.3: Gap Analysis & Hardening Plan

**Version:** 1.0
**Basis:** `niwlekakan/voidtower` @ `main` (v0.9.0, cloned 2026-07-08), read against *VoidTower HomeOS EDD v0.3*
**Relationship to the EDD:** this document **supersedes EDD §2 (workspace layout), §6 (schema, as greenfield), §16 (roadmap)**. EDD §3.2/§10 (Voidwatch semantics), §4 (agent protocol), §8 (plugin SDK, partially), and §15 (agent-team development model) remain in force as target specifications, retargeted per this plan.

---

## 1. Verdict

The EDD assumed a greenfield project where implementation was the work. The repo is the opposite: **a v0.9 product with a feature surface already exceeding the EDD's 1.0 scope, and verification near zero.** Confirmed against source:

| Dimension | EDD assumption | Repo reality |
|---|---|---|
| Codebase | Empty workspace to scaffold | Single crate `voidtower` 0.9.0, ~75K LOC (78 backend .rs files + large React frontend) |
| Features | 5 apps, narrow T0 | 52 App Vault apps, Proxmox + libvirt, restic w/ restore confidence, firewall, WireGuard, RAID, plugins, OIDC, TOTP, RBAC (4 roles), disaster recovery, in-UI self-update, 23 themes |
| Odysseus | In-process crate | **Separate repo/service** (`odysseus-voidlink`), Docker network, bearer token + webhook secret; "Voidwatch" = the integration layer, not a policy engine |
| Policy | Mode ladder, default-deny for AI, denylist, before AI ships | `policy.rs`: flat deny-rules, **default-allow**, 3 call sites (services, containers, webhook automations). **MCP `tools/call` and Studio `mcp_invoke` are not gated** (confirmed in code and admitted in ROADMAP.md) |
| Tests | Gates G0–G4, mutation testing | **2 files with `#[test]`** in the entire tree; CI = clippy `-D warnings` + frontend lint/build |
| Schema | sqlx migration files, forbidden zone | Ad-hoc `CREATE TABLE IF NOT EXISTS` accretion in `db/mod.rs` (466 lines); no migration files |
| Multi-node | Device agent T0 | `--agent` flag parsed, unimplemented; single-host, manages its own host via mounted `docker.sock` |

**Interpretation.** This codebase is what high-throughput agent-driven development produces without the §15 gate structure: enormous, functional, demo-complete breadth; near-absent tests; the security boundary (policy on the AI path) left as the one feature that's hard to demo. The EDD's development model is not invalidated by this repo — it is *evidenced* by it. Going forward, §15 (gates, forbidden zones, forge, adversarial review) applies to every change listed below.

**Strategic call:** do **not** restructure the working single crate into the EDD's workspace, and do **not** merge Odysseus in-process. Both existing shapes are acceptable. All effort goes into four priorities, in order.

---

## 2. P0 — Close the AI blast radius *(highest urgency, small surface)* — **STATUS: COMPLETE (2026-07-11)**

All six work items below are merged to `main`. P0.3's frontend/approvals-queue half (making mode-ladder verdicts mandatory, not advisory, at the six UI-driven handlers) was deliberately deferred — see [issue #11](https://github.com/niwlekakan/voidtower/issues/11) — because it needs an approvals-queue mechanism ADR-002 explicitly scoped out of this phase; the backend mode ladder itself shipped. Phase-exit (`.devteam/phase-exit/P0.ok`) is a software review against the exit criteria below, not a hardware checklist — VoidTower runs on any hardware.

**The chain to break:** prompt-injected Odysseus → MCP/integration API using `VOIDTOWER_TOKEN` → un-gated action endpoints → host with `docker.sock` mounted (root-equivalent) → includes VoidTower's own self-update mechanism.

Work items, each small and independently shippable:

**P0.1 — Single AI ingress choke point.** Inventory every endpoint reachable with the Voidwatch/integration token (`api/mcp.rs`, `api/integrations.rs`, `api/studio.rs` `mcp_invoke`, `api/ai_ask.rs`, webhooks). Route every mutating action through one function: `voidwatch::evaluate(actor, action, resource) -> Verdict`. No AI-originated code path may reach an action handler without passing it. This is EDD §3.2 made structural in the existing code.

**P0.2 — Default-deny for AI actors.** `policy::check` currently returns Allow when no rule matches. Split verdict semantics by actor class: `user` sessions keep RBAC behavior; `api_token` / `automation` / new `ai` actor class flip to **default-deny with an explicit allowlist**. Migration ships with a generated allowlist matching current observed usage so nothing breaks on upgrade, but the *default* for new actions is deny.

**P0.3 — Mode ladder + risk classes on top of `policy_rules`.** Implement Observer / Assisted / Trusted / YOLO exactly per EDD §3.2, as a pre-pass before rule evaluation. Every action name in the API surface gets a `risk_class` (read | mutate | destructive | irreversible) in a compile-time table — clippy-style exhaustiveness so a new endpoint cannot ship unclassified. Approvals reuse the existing `ChangePlanModal` pattern, which is already the right UX primitive — it just needs to become *mandatory by verdict* rather than advisory by convention.

**P0.4 — Irreversibility denylist (EDD §10.3), hardcoded.** Regardless of mode: self-update trigger, disaster-recovery import/reset, secrets export, policy/mode edits, `docker.sock`-level raw operations, disk wipe/format, firewall disable. These always require a human approval, and the denylist itself is not editable via API.

**P0.5 — Secrets redaction on the AI context path.** ROADMAP.md already names this gap ("full redaction from Odysseus context bundles"). Redaction middleware on every response serialized into MCP/integration/context-bundle output, with a test corpus of known secret values that must never appear (this test is P1's first citizen).

**P0.6 — Scope down `VOIDTOWER_TOKEN`.** One god-token for the AI stack is the current model. Split into per-capability scoped tokens (read, deploy, exec, admin-never), minted from the existing scoped API token system, so mode ladder + scopes compose.

**Exit criteria:** the policy mode×risk×actor matrix is table-tested exhaustively; a seeded prompt-injection eval (EDD §9.4) attempting each denylist item via MCP fails at the choke point; redaction corpus test green. **Feature freeze until P0 exits.**

---

## 3. P1 — Verification retrofit *(the debt, paid strategically)*

Writing tests for 75K LOC uniformly is neither possible nor useful. Retrofit where failure is expensive, gate-trust the rest:

| Target | Mechanism | Rationale |
|---|---|---|
| Policy/Voidwatch (P0 output) | Exhaustive matrix tests + `cargo-mutants` | The security boundary gets the EDD's full G1 treatment |
| Auth (sessions, TOTP, OIDC, token scopes) | Authz matrix test: enumerate the router (~60 API modules), assert every route declares required role/scope; unauthenticated + wrong-role probes generated per route | Catches the "new endpoint forgot auth" class mechanically, forever |
| Secrets | Redaction corpus (P0.5) + reveal-audit invariant tests | Highest-consequence data |
| Destructive operations | Every action classified `destructive`/`irreversible` must have a change-plan + approval test | Aligns UX pattern with policy engine |
| Golden paths | CI integration job (Docker-in-Docker): deploy one App Vault app end-to-end; restic backup → restore-test; container lifecycle | Proves the product's core promise per commit |
| Schema | Fresh-DB schema dump golden file + upgrade test from a seeded v0.9.0 database | Prerequisite for P2 |
| Everything else | Gate-trusted per EDD §15.5 | Coverage where it pays, honestly nowhere else |

Also in P1: adopt the EDD §12 CI additions (cargo-deny/audit, forbidden-zone path protection via CODEOWNERS — which now protects `policy.rs`/`voidwatch`, auth, secrets, `db/mod.rs`, CI itself) and stand up the **forge VM** (EDD §15.6) so all subsequent agent work happens sandboxed, with the nested-PVE fixture environment for the Proxmox surface.

**Exit criteria:** CI blocks on the authz matrix, policy matrix, redaction corpus, golden-path integration, and schema golden file. From this point, EDD §15 gates apply to all merges.

---

## 4. P2 — Schema discipline

Convert `db/mod.rs` accretion to sqlx migration files: migration `0000_baseline.sql` = exact current v0.9.0 schema (generated from the golden dump), all future changes as numbered migrations, `db/mod.rs` reduced to pool setup + `sqlx::migrate!`. Upgrade test from P1 guards the conversion. Schema enters the forbidden zones. Small, boring, prevents the first real data-loss incident.

---

## 5. P3 — Device agent & multi-node *(the actual feature gap)*

This is the only large *new* build remaining between VoidTower AIO and the HomeOS vision, and the `--agent` flag stub is its natural entry point. **EDD §4 (`vt-proto` v1) is the spec, unchanged:** agent-initiated WSS, mTLS enrollment with join tokens and an internal CA, fixed verb set (`exec.run`, `docker.proxy`, `svc.action`, `file.*`, `sys.inventory`, metrics), no agent-side plugins. Implementation notes adjusted to reality:

- The agent ships as a **second binary target in the existing crate** (or a tiny sibling crate sharing only protocol types) — workspace purity is not worth a restructure; binary-size discipline is.
- Existing local-host management modules (containers via bollard, services, files, terminal) refactor incrementally behind a `HostHandle` trait with two impls: `Local` (today's behavior, zero regression) and `Remote(agent)`. This is the *only* structural refactor this plan endorses, because it's the one that pays.
- Remote hosts appear through the existing tags/policy/timeline systems; per-device mode overrides (EDD §3.2) apply from day one because P0 shipped first.

**Exit criteria:** protocol conformance suite (EDD §13) green; a second real machine (post-migration sessrumnir) enrolled and managed in production for two weeks, including a control-plane restart and an agent update rollout with health gate.

---

## 6. P4 — Deliberate non-actions

Recorded so they don't resurface as drift:

- **No workspace big-bang.** The single crate stays; module boundaries harden via the `HostHandle` seam and CODEOWNERS, not via a rewrite that would invalidate 75K LOC of working behavior with zero tests to catch regressions during the move.
- **No Odysseus in-process merge.** The separate-service + scoped-token model is architecturally sound *once P0 lands*; the compose `aio` profile is a good deployment story.
- **No new features during P0/P1.** Including from the T1 backlog. The repo's problem is not missing features.
- **T2 remains T2.** Kubernetes, mesh scheduling, voice, gaming layer — unchanged verdict; the codebase having momentum does not change their maintenance-surface economics.

---

## 7. Sequencing & effort (agent-team model, §15 gates active)

| Phase | Calendar (5–10 architect h/wk) | Parallelizable? |
|---|---|---|
| P0 blast radius | 2–3 weeks | P0.1–P0.6 largely parallel after the choke-point design (one day, human-written) |
| P1 verification | 2–4 weeks, overlaps P0 tail | Highly — each row is an independent track |
| P2 schema | 1 week | Serial, after P1 schema golden file |
| P3 device agent | 4–6 weeks | Protocol/agent/`HostHandle` refactor as three tracks |

**Total ≈ 2–3.5 months to a hardened multi-node 1.0** — comparable to the EDD's greenfield estimate, which is the whole point: the repo bought breadth early and deferred exactly the work that cannot be deferred to agents alone. Order is non-negotiable: P0 before any further AI-facing capability, P1 before P3 (retrofitting a protocol without a conformance suite recreates the current situation one layer deeper).
