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
