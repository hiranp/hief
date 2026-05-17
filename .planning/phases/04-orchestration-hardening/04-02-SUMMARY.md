---
phase: 04-orchestration-hardening
plan: 02
subsystem: orchestration
tags: [worktree-scope, session-memory, telemetry, intent-locks]
requires:
  - phase: 04-orchestration-hardening
    provides: fail-closed eval gate baseline from 04-01
provides:
  - worktree-scoped session telemetry and cognitive memory retrieval
  - deterministic MCP worktree identity propagation for session operations
  - intent soft-lock acquisition, conflict rejection, release, and expiry reclaim
affects: [db, mcp-tools, graph, session-context, intent-ownership]
tech-stack:
  added: []
  patterns: [worktree partition keys, soft-lock lease ownership, scoped status transitions]
key-files:
  created: [tests/test_session_scope.rs, tests/test_intent_soft_locks.rs]
  modified: [src/db.rs, src/index/memory.rs, src/mcp/tools.rs, src/mcp/resources.rs, src/graph/intent.rs, src/graph/mod.rs, src/errors.rs]
key-decisions:
  - "Use explicit scoped APIs with project-root fallback to preserve backwards compatibility for callers without worktree IDs."
  - "Acquire soft locks before in_progress status writes and release them when leaving in_progress to prevent cross-worktree ownership races."
patterns-established:
  - "MCP server computes deterministic worktree_id from canonical project root and passes it to telemetry and session memory paths."
  - "Intent lock conflicts return typed IntentLockConflict with holder and worktree ownership context."
requirements-completed: [ORG-02, ORG-03]
duration: 5m
completed: 2026-05-17
---

# Phase 04 Plan 02: Worktree-Scoped Sessions and Intent Soft Locks Summary

**Worktree-partitioned session telemetry/memory and lease-based intent soft locks now prevent cross-worktree context pollution and in_progress ownership collisions.**

## Performance

- **Duration:** 5 min
- **Started:** 2026-05-17T04:27:19Z
- **Completed:** 2026-05-17T04:32:08Z
- **Tasks:** 2
- **Files modified:** 9

## Accomplishments

- Added worktree partitioning for `chunk_access` and `tool_events` with scoped DB query/write APIs.
- Wired MCP session telemetry and session-context access through deterministic per-worktree IDs.
- Added `intent_locks` table and lock helpers with conflict rejection, release, and lease expiry reclaim.
- Enforced lock acquisition on transition to `in_progress` and lock release when leaving `in_progress`.
- Added integration tests for session isolation and intent soft-lock behavior.

## Task Commits

Each task was committed atomically:

1. **Task 1: Scope session telemetry and memory by worktree** - `479e46a` (test), `544382c` (feat)
2. **Task 2: Add intent soft-lock ownership checks** - `f461c60` (test), `6e69282` (feat)

**Plan metadata:** pending final docs commit

## Files Created/Modified

- `tests/test_session_scope.rs` - Integration coverage for cross-worktree session context and telemetry isolation.
- `src/db.rs` - Added scoped telemetry methods and migrations `007_worktree_scope` + `008_intent_locks`.
- `src/index/memory.rs` - Added scoped memory read/write helpers with worktree filtering.
- `src/mcp/tools.rs` - Added deterministic `worktree_id` derivation and scoped propagation for memory/telemetry and status updates.
- `src/mcp/resources.rs` - Session summary now queries telemetry by worktree scope.
- `tests/test_intent_soft_locks.rs` - Integration coverage for lock acquisition, conflict, release, and expiry reclaim.
- `src/graph/intent.rs` - Implemented `acquire_soft_lock` and `release_soft_lock` helpers.
- `src/graph/mod.rs` - Added `update_status_scoped` with lock lifecycle around status transitions.
- `src/errors.rs` - Added typed `IntentLockConflict` error.

## Decisions Made

- Reused the stale timeout hours as lock lease duration to align lock expiry with existing stale intent recovery behavior.
- Preserved old non-scoped APIs as wrappers so legacy callers degrade to `project-root` scope safely.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 2 - Missing Critical] Scoped session summary resource to worktree context**

- **Found during:** Task 1
- **Issue:** Session telemetry writes were scoped, but `get_session_summary` still read unscoped aggregates, which could hide scoped writes from callers.
- **Fix:** Added scoped session summary query path and propagated worktree context through MCP resource call.
- **Files modified:** `src/mcp/resources.rs`, `src/mcp/tools.rs`, `src/db.rs`
- **Verification:** `cargo test --test test_session_scope -- --nocapture`
- **Committed in:** `544382c`

---

**Total deviations:** 1 auto-fixed (1 missing critical)
**Impact on plan:** No scope creep; change was required to keep session telemetry behavior consistent with scoped writes.

## Issues Encountered

- Initial lock expiry test attempted reclaim against a non-expired lease.
- Adjusted test flow to create an immediately expiring lease first, then verify reclaim from another worktree.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- Plan 04-02 outcomes are complete and verified.
- Plan 04-03 can now build retrieval learning on top of isolated session telemetry and lock-safe intent orchestration.

## Self-Check: PASSED

- Found summary file: `.planning/phases/04-orchestration-hardening/04-02-SUMMARY.md`
- Found commits: `479e46a`, `544382c`, `f461c60`, `6e69282`
