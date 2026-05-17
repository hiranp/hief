# ROADMAP

## Planning Baseline

This roadmap converts research in .planning/research into executable plan files.
Each phase includes explicit requirement IDs to enforce traceability.

## Status Key

- [ ] not started
- [~] in progress
- [x] complete

---

### Phase 01: Retrieval Governance

**Goal:** Make retrieval deterministic, token-budgeted, and compression-aware so tool responses remain grounded under context pressure.

**Requirements:** [RET-01, RET-02, RET-03, RET-04]

**Prerequisite note:** `src/index/vectors.rs` has LanceDB wired but `enabled: false` by default. Plan 01-02 Task 1 must explicitly enable vector search and verify LanceDB table initialization before adding the semantic cache layer.

Plans:

- [x] 01-01-PLAN.md - adaptive retrieval routing and response budget enforcement
- [x] 01-02-PLAN.md - semantic cache and retrieval quality instrumentation (depends on 01-01; activates LanceDB)

---

### Phase 02: Protocol Hardening

**Goal:** Reduce agent/tool failure modes via strict contracts, retry hints, and safe platform integration.

**Requirements:** [PRO-01, PRO-02, PRO-03, PRO-04]

**Two-phase delivery note (PRO-03):** Plan 02-02 delivers deterministic lane routing logic and explainable route decisions. The audit-trail write path (persisting lane+reason to `tool_events`) is intentionally deferred to Phase 03-02 where the `tool_events` schema exists. PRO-03 is fully satisfied only after both Phase 02 and Phase 03-02 complete. The 02-02 done criteria must reflect this split.

**Scope note (PRO-04):** Plan 02-02 Task 2 delivers CLI argument parsing and dry-run output for `hief install --platform`. Full registration logic is Phase 02 scope; actual platform-specific config writes are deferred to a follow-on plan. Update FEATURES.md to reclassify Cross-Platform Auto-Registration to P1 until the real install logic ships.

Plans:

- [x] 02-01-PLAN.md - strict MCP validation and retry-hint responses
- [x] 02-02-PLAN.md - protocol lane router and platform installer scaffolding

---

### Phase 03: Observability and Active Eval Loop

**Goal:** Turn evaluation into an active feedback loop with durable telemetry and per-query groundedness.

**Requirements:** [OBS-01, OBS-02, EVAL-01, EVAL-02]

**EVAL-01 ownership:** EVAL-01 is owned entirely by Phase 03-03 (groundedness scorer). Plan 01-02 Task 2 reserves a `quality_signal` field slot with a placeholder value only — it does NOT emit a scored groundedness value. This prevents the hollow-stub anti-pattern where Phase 01 defines a data contract before Phase 03 implements it.

**EVAL-02 scope:** Plan 03-03 stores trajectory rows that enable retrieval-weight tuning. The weight-adjustment feedback loop (LEARN phase of PAVL) is implemented in Phase 05 (Orchestration Hardening) once sufficient trajectory history exists.

Plans:

- [x] 03-01-PLAN.md - telemetry schema and write path
- [x] 03-02-PLAN.md - MCP instrumentation, session-cost tooling, and PRO-03 lane-event audit wiring
- [x] 03-03-PLAN.md - groundedness scoring and trajectory capture

---

### Phase 04: Orchestration Hardening

**Goal:** Enforce wave-level verification gates, add worktree-scoped session isolation, and prevent parallel regression amplification in multi-agent workflows.

**Requirements:** [ORG-01, ORG-02, ORG-03]

**Research basis:** ARCHITECTURE.md Plane 3 (Coordination), AGENTIC-TRENDS.md Trend 2 (Multi-Agent Coordination), PITFALLS.md Pitfall 2 (Verification Gate Bypass), ARCHITECT-REVIEW.md finding 3 (High severity).

Plans:

- [x] 04-01-PLAN.md - wave-level eval gate enforcement and fail-closed verification in intent state machine
- [x] 04-02-PLAN.md - worktree-scoped session state and intent soft locks
- [x] 04-03-PLAN.md - retrieval-weight adjustment from trajectory history (PAVL LEARN phase, closes EVAL-02)

---

### Phase 05: Memory Versioning

**Goal:** Add write provenance, per-intent rollback, and TTL-bounded decay so memory drift and stale context contamination become detectable and recoverable.

**Requirements:** [MEM-01, MEM-02, MEM-03, MEM-04]

**Research basis:** ARCHITECTURE.md Plane 2 (Memory), PITFALLS.md Pitfall 1 (Memory Drift — Critical Risk), AGENTIC-TRENDS.md Trend 4 (Expanding Task Horizons).

Plans:

- [ ] 05-01-PLAN.md - write provenance metadata schema and per-intent rollback surface
- [ ] 05-02-PLAN.md - memory tier manager and TTL decay policy enforcement
- [ ] 05-03-PLAN.md - background consolidation worker (short-term → long-term tier promotion)

---

## Requirement Glossary

### Retrieval

- RET-01: Route retrieval by query intent (deterministic, hybrid, semantic).
- RET-02: Enforce per-tool token budget.
- RET-03: Compress results without losing failure-critical information.
- RET-04: Cache semantically similar queries with bounded staleness.

### Protocol

- PRO-01: Validate MCP inputs with strict schemas and explicit error classes.
- PRO-02: Return correction-hint payloads for recoverable tool-call failures.
- PRO-03: Route operations through CLI or MCP policy lanes with persisted audit reason. *(Phase 02 delivers routing logic; Phase 03-02 delivers audit-trail write path.)*
- PRO-04: Support automated MCP platform registration command surface.

### Observability

- OBS-01: Persist tool invocation telemetry in SQL with queryable summaries.
- OBS-02: Expose per-session cost and quality snapshots through CLI/MCP.

### Eval

- EVAL-01: Record groundedness score per retrieval call. *(Owned by Phase 03-03.)*
- EVAL-02: Store trajectory rows to enable retrieval-weight tuning. *(Storage in Phase 03-03; LEARN-phase weight adjustment in Phase 04-03.)*

### Orchestration

- ORG-01: Block wave advancement on eval-gate failure (fail-closed verification).
- ORG-02: Scope session and access records to active git worktree path.
- ORG-03: Maintain intent-level soft locks for shared-branch coordination.

### Memory

- MEM-01: Record write provenance (who, when, source_intent, content_hash) on every memory write.
- MEM-02: Support per-intent and per-session memory rollback.
- MEM-03: Enforce TTL decay policies per memory tier (working / short-term / long-term / archival).
- MEM-04: Background consolidation of short-term session patterns into long-term project memory.
