---
status: complete
phase: 03-observability-eval-loop
source: [03-01-SUMMARY.md, 03-02-SUMMARY.md, 03-03-SUMMARY.md]
started: 2026-05-17T03:42:00Z
updated: 2026-05-17T03:45:00Z
---

## Current Test

[testing complete]

## Tests

### 1. Cold Start Smoke Test
expected: |
  Fresh `cargo build` completes with migration 006 included. Running
  `hief serve` initialises the DB and the tool_events table is present.
result: pass

### 2. Migration Idempotency
expected: |
  Running migrations twice on the same database does not error and leaves
  the schema unchanged after the second run.
result: pass

### 3. tool_events Table Schema
expected: |
  `tool_events` table has columns: session_id, tool, query, strategy,
  result_count, latency_ms, groundedness_score, created_at. All types
  and NOT NULL constraints match the OBS-01 spec.
result: pass

### 4. session_summary VIEW Aggregation
expected: |
  Inserting multiple tool_events for session "s1" and querying the VIEW
  returns correct totals for total_events, avg_latency_ms, avg_groundedness,
  session_start, session_end, and session_duration_seconds.
result: pass

### 5. record_tool_event Optional Fields Map to NULL
expected: |
  Calling record_tool_event with strategy=None, result_count=None,
  groundedness_score=None stores NULL (not empty string) in those columns.
result: pass

### 6. Telemetry Instrumented in search_code
expected: |
  After a search_code call the tool_events table contains a row for that
  session with tool="search_code", non-null latency_ms, and the lane/reason
  encoded in the strategy column.
result: pass

### 7. Telemetry Failure is Non-Fatal
expected: |
  If the telemetry write errors (e.g., DB locked), the search_code handler
  still returns its result payload to the caller with no error.
result: pass

### 8. get_session_summary MCP Tool — Populated Session
expected: |
  Calling `get_session_summary` with a session_id that has events returns
  total_calls, total_latency_ms, avg_groundedness, and a per-tool breakdown
  without leaking raw query strings.
result: pass

### 9. get_session_summary MCP Tool — Empty Session
expected: |
  Calling `get_session_summary` for a session with no events returns zero
  values (not null/error): total_calls=0, avg_groundedness=0.0, etc.
result: pass

### 10. hief session-cost CLI — Human-Readable Output
expected: |
  `hief session-cost --session-id S` prints a readable summary with per-tool
  call counts and average latency. `--json` flag switches to machine-readable
  JSON output.
result: pass

### 11. Groundedness Score — Deterministic and Bounded
expected: |
  `groundedness_score("search_code", ["search_code searches code"])` returns
  a value in [0.0, 1.0]. Same inputs always produce the same output.
result: pass

### 12. Groundedness Score — Empty/Low-Signal Safety
expected: |
  Calling groundedness_score with an empty query or empty contents array
  returns 0.0 without panicking.
result: pass

### 13. SearchResult Carries groundedness_score
expected: |
  After a search_code call each SearchResult in the response has
  `groundedness_score` populated (not null). The value reflects lexical
  overlap between the query and the returned content.
result: pass

### 14. Trajectory Fields Persisted to tool_events
expected: |
  The tool_events row for a search_code call stores: query text, strategy
  with lane+reason, result_count, groundedness_score, and session_id.
  All five fields are non-null for a successful retrieval.
result: pass

## Summary

total: 14
passed: 14
issues: 0
pending: 0
skipped: 0

## Gaps

[none]
