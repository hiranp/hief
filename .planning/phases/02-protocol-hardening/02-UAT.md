---
status: complete
phase: 02-protocol-hardening
source: [02-01-SUMMARY.md, 02-02-SUMMARY.md]
started: 2026-05-17T03:42:00Z
updated: 2026-05-17T03:45:00Z
---

## Current Test

[testing complete]

## Tests

### 1. Cold Start Smoke Test
expected: |
  `hief serve` starts (or cargo build succeeds) with the new validation and
  CLI additions compiled in — no panics or missing symbol errors.
result: pass

### 2. Missing Required Parameter Error Shape
expected: |
  Calling search_code without `query` returns an MCP error with
  `error_class: "invalid_params"`, `parameter: "query"`, `recoverable: true`,
  and a `correction_hint` action string.
result: pass

### 3. top_k Out-of-Bounds Error Shape
expected: |
  Passing `top_k: 0` or `top_k: 1001` returns a structured constraint error
  with `min: 1`, `max: 1000`, and a numeric example in the correction_hint.
result: pass

### 4. Path Traversal Rejection
expected: |
  Passing `file: "../../../etc/passwd"` to git_blame returns a security error
  payload. The error message does NOT echo the attacker path back to the caller.
result: pass

### 5. Valid Parameters Pass Through
expected: |
  Valid combinations (non-empty query, 1 ≤ top_k ≤ 1000, relative file path)
  proceed to normal handler execution without triggering any validation error.
result: pass

### 6. Protocol Lane — Token Pressure Routes to progressive-mcp
expected: |
  `select_lane` with `estimated_tokens` above the configured threshold returns
  `lane: "progressive-mcp"` with an explanatory reason string.
result: pass

### 7. Protocol Lane — Remote Auth Routes to mcp
expected: |
  `select_lane` with `remote_auth_required: true` and tokens below threshold
  returns `lane: "mcp"`.
result: pass

### 8. Protocol Lane — Local Deterministic Routes to cli
expected: |
  `select_lane` with `local_deterministic: true`, no auth, tokens below
  threshold returns `lane: "cli"`.
result: pass

### 9. hief install --platform dry-run Preview
expected: |
  `hief install --platform cursor` prints a deterministic TOML/JSON config
  block to stdout. No files are written to disk.
result: pass

### 10. hief install --platform Invalid Platform
expected: |
  `hief install --platform nonexistent` returns a typed constraint error
  reusing the same ToolValidationPayload style used by MCP validation.
result: pass

## Summary

total: 10
passed: 10
issues: 0
pending: 0
skipped: 0

## Gaps

[none]
