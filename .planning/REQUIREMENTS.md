# Requirements

## Retrieval

- [x] **RET-01** Route retrieval by query intent (deterministic, hybrid, semantic).
- [x] **RET-02** Enforce per-tool token budget.
- [x] **RET-03** Compress results without losing failure-critical information.
- [x] **RET-04** Cache semantically similar queries with bounded staleness.

## Protocol

- [x] **PRO-01** Validate MCP inputs with strict schemas and explicit error classes.
- [x] **PRO-02** Return correction-hint payloads for recoverable tool-call failures.
- [x] **PRO-03** Route operations through CLI or MCP policy lanes with persisted audit reason.
- [x] **PRO-04** Support automated MCP platform registration command surface.

## Observability

- [x] **OBS-01** Persist tool invocation telemetry and session-level SQL summaries.
- [x] **OBS-02** Instrument MCP handlers and expose session summary product surfaces.

## Evaluation Loop

- [x] **EVAL-01** Compute deterministic L0 groundedness scores for retrieval responses.
- [x] **EVAL-02** Persist retrieval trajectories linking query, strategy, score, and outcomes.

## Orchestration

- [x] **ORG-01** Block wave advancement on eval-gate failure (fail-closed verification).
- [x] **ORG-02** Scope session and access records to active git worktree path.
- [x] **ORG-03** Maintain intent-level soft locks for shared-branch coordination.

## UI

- [x] **UI-01** Expose a read-only Axum/Askama dashboard showing active intents and worktree projections.
- [x] **UI-02** Stream live agent activity and telemetry updates using Server-Sent Events (SSE).
- [x] **UI-03** Manage Git worktree lifecycle using async tokio::process::Command without blocking the HTTP server.
- [x] **UI-04** Present a detailed task view including PAVL state, token costs, and HITL block controls.

## Traceability Table

| Requirement | Scope | Status | Notes |
| --- | --- | --- | --- |
| RET-01 | Phase 01 retrieval routing | Complete | Query shape routing for search lanes |
| RET-02 | Phase 01 response budget | Complete | Deterministic search payload boundary |
| RET-03 | Phase 01 compression | Complete | Preserve metadata and snippets |
| RET-04 | Phase 01 semantic cache | Complete | TTL-bounded cache for similar queries |
| PRO-01 | Phase 02 MCP validation boundary | Complete | Typed invalid-params and security failures |
| PRO-02 | Phase 02 retry-hint responses | Complete | Deterministic correction hints for recoverable errors |
| PRO-03 | Phase 02 protocol lane routing | Complete | Audit-trail persistence deferred to Phase 03-02 |
| PRO-04 | Phase 02 installer command surface | Complete | Dry-run preview shipped; real writes deferred |
| OBS-01 | Phase 03 telemetry schema and write path | Complete | tool_events table + session_summary view + write APIs |
| OBS-02 | Phase 03 MCP instrumentation and summary surfaces | Complete | best-effort telemetry hooks + session summary resource + CLI session-cost |
| EVAL-01 | Phase 03 deterministic groundedness scoring | Complete | lexical overlap score in [0,1] for lexical and semantic retrieval |
| EVAL-02 | Phase 03 retrieval trajectory persistence | Complete | query + strategy + score + result_count + session persisted in tool_events |
| ORG-01 | Phase 04 wave-level eval gating | Complete | fail-closed transitions in_review->verified and verified->merged |
| ORG-02 | Phase 04 worktree-scoped telemetry and memory | Complete | scope partitioning by deterministic worktree_id |
| ORG-03 | Phase 04 intent soft locks | Complete | lease-based lock ownership on in_progress transitions |
| UI-01 | Phase 05 dashboard foundation | Complete | read-only intent/worktree dashboard via Axum + Askama |
| UI-02 | Phase 05 live activity surface | Complete | SSE stream + HTMX reconnect-safe activity fragment |
| UI-03 | Phase 05 async worktree operations | Complete | async git adapter + deterministic intent/worktree binding |
| UI-04 | Phase 05 task detail and HITL controls | Complete | task detail, review panel, and validated block/unblock actions |
