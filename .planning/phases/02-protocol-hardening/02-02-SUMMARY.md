---
phase: 02-protocol-hardening
plan: 02
subsystem: cli
tags: [router, cli, mcp, install, dry-run]
requires:
  - phase: 02-protocol-hardening
    provides: typed MCP validation patterns reused for install-platform errors
provides:
  - deterministic protocol lane selector with explainable reason strings
  - install command surface with validated platform parsing and dry-run preview output
affects: [cli, protocol-hardening, observability]
tech-stack:
  added: []
  patterns: [lane selection with reason strings, deferred install previews]
key-files:
  created: [tests/test_protocol_router.rs]
  modified: [src/router/mod.rs, src/config.rs, src/cli/mod.rs, src/cli/commands/mod.rs, src/main.rs]
key-decisions:
  - "Route token-pressure operations to a progressive MCP lane before auth-vs-local checks so large payload risk is deterministic."
  - "Ship `hief install` as a dry-run preview first and defer real config writes to a follow-on plan to avoid unsafe early mutation."
patterns-established:
  - "Protocol lane selection returns both lane and reason for auditable routing decisions."
  - "Installer entrypoints accept `--dry-run=false` but mark the action deferred until the real write path exists."
requirements-completed: [PRO-03, PRO-04]
duration: 30m
completed: 2026-05-17
---

# Phase 02 Plan 02: Protocol Lane Router and Installer Scaffolding Summary

**Deterministic CLI, MCP, and progressive-MCP lane routing with a validated `hief install` dry-run preview for editor platform registration**

## Performance

- **Duration:** 30 min
- **Started:** 2026-05-17T02:40:00Z
- **Completed:** 2026-05-17T03:09:58Z
- **Tasks:** 2
- **Files modified:** 6

## Accomplishments

- Added protocol operation routing with explicit `cli`, `mcp`, and `progressive-mcp` lanes plus deterministic reason strings.
- Added router config hooks for default lane and token-pressure threshold.
- Added a top-level `hief install --platform ...` command that validates target platforms and prints a deterministic dry-run config block.

## Task Commits

1. **Task 1 + Task 2: lane selector and installer scaffolding** - `00b9920` (feat)

## Files Created/Modified

- `src/router/mod.rs` - Added protocol lane enums, operation request model, and deterministic `select_lane` logic.
- `src/config.rs` - Added default lane and token-pressure router settings.
- `src/cli/mod.rs` - Added top-level `install` command arguments.
- `src/cli/commands/mod.rs` - Added platform parsing, install preview generation, and deferred dry-run execution path.
- `src/main.rs` - Wired the new install command into CLI dispatch.
- `tests/test_protocol_router.rs` - Added integration tests for lane selection and install command parsing.

## Decisions Made

- Prioritized token-pressure routing over other conditions so large operations always take the progressive lane regardless of local/remote flags.
- Reused the existing error-constraint style for invalid install platforms instead of adding a separate CLI-only error type.

## Deviations from Plan

None - plan executed exactly as written.

## Known Stubs

- `src/cli/commands/mod.rs`: `// TODO(PRO-04): implement real registration write path.`
  Reason: The plan explicitly deferred platform-specific config writes to a follow-on plan, so this command intentionally stops at deterministic preview output.

## Issues Encountered

- The new CLI tests initially attempted to format `Commands` with `Debug`; simplified the panic branches because the enum does not derive `Debug`.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- Phase 03-02 can attach lane-decision audit logging to the existing `LaneDecision` reason strings.
- Future platform-write work can extend `build_install_preview` into a real registration executor without changing CLI parsing.

## Self-Check: PASSED

- Summary file created.
- Commit `00b9920` verified in git history.

---
*Phase: 02-protocol-hardening*
*Completed: 2026-05-17*
