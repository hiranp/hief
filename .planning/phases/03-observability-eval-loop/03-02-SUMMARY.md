---
phase: 03-observability-eval-loop
plan: 02
subsystem: mcp-observability
tags: [obs-02, telemetry, mcp, cli]
requires:
  - phase: 03-observability-eval-loop
    provides: tool_events schema from 03-01
provides:
  - best-effort telemetry instrumentation in MCP retrieval handlers
  - session summary surface via MCP get_session_summary tool
  - session-cost CLI command for aggregate telemetry readout
affects: [mcp, cli, db, observability]
tech-stack:
  added: []
  patterns: [best-effort telemetry writes, aggregate-only summary output]
key-files:
  created: []
  modified:
    - src/mcp/tools.rs
    - src/mcp/resources.rs
    - src/cli/mod.rs
    - src/cli/commands/mod.rs
    - src/main.rs
    - src/db.rs
    - tests/test_tool_events.rs
key-decisions:
  - "Telemetry failures are non-fatal by design: writes are best-effort and never block primary MCP behavior."
  - "PRO-03 lane audit trail is encoded into strategy text with lane, reason, and outcome to avoid schema-breaking migrations."
  - "Session summary output is aggregate-only to avoid leaking raw query content by default."
requirements-completed: [OBS-02]
duration: 40m
completed: 2026-05-16
---

# Phase 03 Plan 02: MCP Instrumentation and Session Cost Tooling Summary

Implemented durable MCP telemetry instrumentation plus user-facing session summary surfaces.

## Accomplishments

- Added telemetry hooks to retrieval MCP handlers:
  - search_code
  - structural_search
  - semantic_search
- Added best-effort write helper so instrumentation failure does not break handler success/error behavior.
- Added protocol lane audit persistence (lane + reason + outcome) in strategy metadata, completing the deferred PRO-03 audit trail linkage from Phase 02.
- Added MCP tool get_session_summary with required session_id and zero-value empty-session behavior.
- Added CLI command surface hief session-cost --session-id SESSION_ID for human-readable or JSON summary output.
- Added DB aggregate API get_session_cost_summary with total_calls, total_latency_ms, avg_groundedness, and per-tool breakdown.

## Test Coverage

- Extended tests/test_tool_events.rs from 8 to 10 tests.
- Added session cost summary aggregate and breakdown validation.
- Added empty-session summary zero-value behavior validation.

Verification commands:

```bash
cargo test --test test_tool_events -- --nocapture
cargo test --test test_config -- --nocapture
```

Result: passed.

## Deviations from Plan

None. Plan executed as written.

## Threat Model Handling

- T-03-03 (DoS on write path): mitigated with best-effort, non-fatal telemetry writes.
- T-03-04 (information disclosure): mitigated with aggregate-only default summary surfaces.

## Known Stubs

None.

## Self-Check: PASSED

- Modified files exist and compile.
- test_tool_events integration suite passes.
- Full repository test suite remains green after implementation.
