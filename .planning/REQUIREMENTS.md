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
