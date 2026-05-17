---
phase: 03-observability-eval-loop
plan: 03
subsystem: eval-loop
tags: [eval-01, eval-02, groundedness, trajectory]
requires:
  - phase: 03-observability-eval-loop
    provides: telemetry persistence primitives from 03-01 and instrumentation from 03-02
provides:
  - deterministic L0 groundedness scoring in eval scorer
  - groundedness signal in lexical and semantic retrieval paths
  - trajectory persistence fields (query, strategy, score, result_count, session)
affects: [eval, index, mcp, telemetry]
tech-stack:
  added: []
  patterns: [local deterministic lexical overlap scoring, additive trajectory capture]
key-files:
  created:
    - tests/test_groundedness.rs
  modified:
    - src/eval/scorer.rs
    - src/index/search.rs
    - src/index/vectors.rs
    - src/mcp/tools.rs
    - tests/test_tool_events.rs
key-decisions:
  - "Groundedness uses deterministic lexical-overlap scoring normalized to [0,1] without external model calls."
  - "Trajectory persistence reuses tool_events schema and strategy encoding to keep migration surface minimal and backward-compatible."
requirements-completed: [EVAL-01, EVAL-02]
duration: 30m
completed: 2026-05-16
---

# Phase 03 Plan 03: Groundedness and Trajectory Capture Summary

Implemented first-loop active evaluation signals by scoring retrieval groundedness and persisting trajectory-ready telemetry.

## Accomplishments

- Added public groundedness helper in eval scorer:
  - groundedness_score(query, contents) -> f64
  - deterministic and bounded in [0.0, 1.0]
- Wired groundedness into lexical retrieval:
  - SearchResult now carries groundedness_score
- Wired groundedness into semantic retrieval:
  - SemanticSearchOutcome now carries groundedness_score
  - semantic response quality_signal is populated from groundedness
- Persisted trajectory-relevant fields through MCP retrieval telemetry:
  - query
  - strategy (includes lane, reason, outcome)
  - result_count
  - groundedness_score
  - session_id

## Test Coverage

Added tests/test_groundedness.rs (4 tests):

- deterministic and bounded scoring behavior
- empty/low-signal safety behavior
- lexical retrieval groundedness population
- trajectory row persistence integrity

Verification commands:

```bash
cargo test --test test_groundedness -- --nocapture
cargo test --test test_eval_workflow -- --nocapture
```

Result: passed.

## Deviations from Plan

None. Plan executed as written.

## Threat Model Handling

- T-03-05 (score tampering risk): mitigated via normalized tokenization and strict clamping.
- T-03-06 (trajectory repudiation): mitigated by session_id + strategy + timestamp persistence for each telemetry event.

## Known Stubs

None.

## Self-Check: PASSED

- Groundedness helper is deterministic and test-covered.
- Retrieval paths emit groundedness signals.
- Trajectory fields are persisted and validated.
- Full repository test suite remains green.
