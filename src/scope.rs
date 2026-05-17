//! Shared utilities for worktree scope normalization and deterministic IDs.

use std::path::Path;

pub const DEFAULT_WORKTREE_SCOPE: &str = "project-root";

/// Normalize optional worktree ID with a stable default scope.
pub fn normalize_worktree_id(worktree_id: Option<&str>) -> String {
    worktree_id
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .unwrap_or(DEFAULT_WORKTREE_SCOPE)
        .to_string()
}

/// Normalize a required string value with a caller-provided default fallback.
pub fn normalize_with_default(value: &str, default: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        default.to_string()
    } else {
        trimmed.to_string()
    }
}

/// Derive a stable worktree fingerprint from canonical project root path.
pub fn derive_worktree_id(project_root: &Path) -> String {
    let canonical = std::fs::canonicalize(project_root)
        .unwrap_or_else(|_| project_root.to_path_buf());
    let fingerprint = blake3::hash(canonical.to_string_lossy().as_bytes());
    format!("wt-{}", &fingerprint.to_hex()[..12])
}