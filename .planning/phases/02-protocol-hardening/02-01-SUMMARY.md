---
phase: 02-protocol-hardening
plan: 01
subsystem: api
tags: [mcp, validation, protocol, retry-hints, security]
requires:
  - phase: 01-retrieval-governance
    provides: retrieval routing and budgeted search handlers reused by MCP tools
provides:
  - typed MCP validation payloads with correction hints
  - strict boundary checks for required params, bounded top_k, and safe relative paths
affects: [mcp, protocol-hardening, tool-contracts]
tech-stack:
  added: []
  patterns: [structured ErrorData.data payloads, deterministic correction hints]
key-files:
  created: [tests/test_mcp_validation.rs]
  modified: [src/errors.rs, src/mcp/mod.rs, src/mcp/tools.rs]
key-decisions:
  - "Use rmcp ErrorData.data for machine-readable validation payloads instead of inventing a parallel error envelope."
  - "Treat empty required strings as boundary validation failures so missing-query retries become deterministic at the handler layer."
patterns-established:
  - "MCP validation errors carry error_class, parameter, reason, and correction_hint fields."
  - "Security-sensitive path failures avoid echoing attacker-controlled path input in messages."
requirements-completed: [PRO-01, PRO-02]
duration: 35m
completed: 2026-05-17
---

# Phase 02 Plan 01: Strict MCP Validation and Retry Hints Summary

Typed MCP validation failures with deterministic retry hints for bounded numeric inputs, required fields, and safe relative paths.

## Performance

- **Duration:** 35 min
- **Started:** 2026-05-17T02:34:00Z
- **Completed:** 2026-05-17T03:09:53Z
- **Tasks:** 2
- **Files modified:** 4

## Accomplishments

- Added reusable MCP validation helpers for required strings, bounded `top_k`, and safe project-relative paths.
- Returned structured `ErrorData.data` payloads with stable error classes and correction hints for recoverable failures.
- Added regression coverage for missing parameters, oversized numeric bounds, traversal attempts, and valid-pass behavior.

## Task Commits

1. **Task 1 + Task 2: validation boundary and retry hints** - `c4c1d58` (feat)

## Files Created/Modified

- `src/errors.rs` - Added explicit tool-validation and tool-security error variants.
- `src/mcp/mod.rs` - Re-exported typed validation payload types for external consumers and tests.
- `src/mcp/tools.rs` - Implemented strict validation helpers and wired search-related handlers through typed failure paths.
- `tests/test_mcp_validation.rs` - Added contract regression tests for invalid params, security failures, and hint shape.

## Decisions Made

- Used `rmcp::ErrorData.data` as the canonical machine-readable error channel so callers get typed metadata without breaking success payloads.
- Kept correction hints deterministic and generic for path errors to avoid reflecting unsafe path input back to the caller.

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered

- The initial verification command used Cargo's test-name filter form and matched zero tests; switched to the target form `cargo test --test test_mcp_validation -- --nocapture` for real execution.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- Protocol lane routing can now assume MCP callers receive stable invalid-parameter feedback.
- Installer and platform entrypoints can reuse the same typed constraint error style.

## Self-Check: PASSED

- Summary file created.
- Commit `c4c1d58` verified in git history.

---
*Phase: 02-protocol-hardening*
*Completed: 2026-05-17*
