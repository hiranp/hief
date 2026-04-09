//! Project-scoped pattern library (.hief/patterns/).
//!
//! Patterns are task-specific guides created from real work in THIS codebase.
//! Unlike skills (generic recipes across any project), patterns capture
//! "how WE do X here" with the gotchas we actually hit, step sequences that
//! match our conventions, and verify checklists specific to our stack.
//!
//! # File format
//!
//! ```markdown
//! # Pattern: Add an API Client
//!
//! ## Steps
//! 1. Create `src/clients/{name}.rs`
//! 2. ...
//!
//! ## Gotchas
//! - Always use `BackoffRetry` — direct calls ignore our retry policy
//!
//! ## Verify
//! - [ ] Integration test passes
//! - [ ] Rate limit headers are forwarded
//! ```
//!
//! INDEX.md is auto-maintained by `sync_index()` and lists all patterns.

use std::path::{Path, PathBuf};

use schemars::JsonSchema;
use serde::Serialize;

use crate::errors::{HiefError, Result};

const PATTERNS_DIR: &str = ".hief/patterns";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Summary of a pattern file (for listing).
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct PatternSummary {
    pub name: String,
    pub title: String,
    pub path: String,
    pub last_modified: Option<i64>,
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn patterns_dir(project_root: &Path) -> PathBuf {
    project_root.join(PATTERNS_DIR)
}

fn validate_name(name: &str) -> Result<()> {
    if name.is_empty() {
        return Err(HiefError::SecurityViolation(
            "pattern name cannot be empty".to_string(),
        ));
    }
    if name.contains('/') || name.contains('\\') || name.contains("..") {
        return Err(HiefError::PathTraversal(format!(
            "pattern name '{}' must be a bare name, not a path",
            name
        )));
    }
    if name.starts_with('.') || name.starts_with('-') {
        return Err(HiefError::SecurityViolation(format!(
            "pattern name '{}' cannot start with '.' or '-'",
            name
        )));
    }
    if !name
        .chars()
        .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
    {
        return Err(HiefError::SecurityViolation(format!(
            "pattern name '{}' must consist of alphanumerics, hyphens, or underscores",
            name
        )));
    }
    Ok(())
}

fn to_filename(name: &str) -> String {
    if name.ends_with(".md") {
        name.to_string()
    } else {
        format!("{}.md", name)
    }
}

fn extract_title(content: &str, fallback: &str) -> String {
    for line in content.lines() {
        if let Some(title) = line.strip_prefix("# ") {
            return title.trim().to_string();
        }
    }
    fallback.to_string()
}

// ---------------------------------------------------------------------------
// List
// ---------------------------------------------------------------------------

/// List all pattern summaries from .hief/patterns/*.md (excluding INDEX.md).
pub fn list_patterns(project_root: &Path) -> Vec<PatternSummary> {
    let dir = patterns_dir(project_root);
    if !dir.exists() {
        return Vec::new();
    }

    let mut entries = Vec::new();
    if let Ok(read_dir) = std::fs::read_dir(&dir) {
        for entry in read_dir.flatten() {
            let path = entry.path();
            let filename = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            if !filename.ends_with(".md") || filename == "INDEX.md" {
                continue;
            }

            let name = filename.trim_end_matches(".md").to_string();
            let content = std::fs::read_to_string(&path).unwrap_or_default();
            let title = extract_title(&content, &name);

            let last_modified = path
                .metadata()
                .ok()
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs() as i64);

            entries.push(PatternSummary {
                name,
                title,
                path: format!("{}/{}", PATTERNS_DIR, filename),
                last_modified,
            });
        }
    }

    entries.sort_by(|a, b| a.name.cmp(&b.name));
    entries
}

// ---------------------------------------------------------------------------
// Get
// ---------------------------------------------------------------------------

/// Get the full markdown content of a pattern by name.
pub fn get_pattern(project_root: &Path, name: &str) -> Result<String> {
    let bare = name.trim_end_matches(".md");
    validate_name(bare)?;
    let path = patterns_dir(project_root).join(to_filename(bare));
    if !path.exists() {
        return Err(HiefError::Other(format!(
            "pattern '{}' not found — use list_patterns to see available patterns",
            name
        )));
    }
    Ok(std::fs::read_to_string(&path)?)
}

// ---------------------------------------------------------------------------
// Create / update
// ---------------------------------------------------------------------------

/// Create or update a pattern file, then regenerate INDEX.md.
///
/// `name` must be a bare identifier like `add-api-client` (no `.md` suffix needed).
/// Content must be non-empty markdown.
pub fn create_pattern(project_root: &Path, name: &str, content: &str) -> Result<()> {
    let bare = name.trim_end_matches(".md");
    validate_name(bare)?;

    if content.trim().is_empty() {
        return Err(HiefError::Other(
            "pattern content is empty — provide steps, gotchas, and a verify checklist".to_string(),
        ));
    }

    let dir = patterns_dir(project_root);
    std::fs::create_dir_all(&dir)?;
    std::fs::write(dir.join(to_filename(bare)), content)?;
    sync_index(project_root)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Index sync
// ---------------------------------------------------------------------------

/// Regenerate .hief/patterns/INDEX.md from all pattern files on disk.
pub fn sync_index(project_root: &Path) -> Result<()> {
    let dir = patterns_dir(project_root);
    if !dir.exists() {
        return Ok(());
    }

    let patterns = list_patterns(project_root);
    if patterns.is_empty() {
        return Ok(());
    }

    let mut lines = vec![
        "# Pattern Index".to_string(),
        String::new(),
        "Task-specific guides built from real work in this codebase.".to_string(),
        String::new(),
        "| Pattern | Purpose |".to_string(),
        "|---------|---------|".to_string(),
    ];

    for p in &patterns {
        lines.push(format!("| [{}]({}.md) | {} |", p.name, p.name, p.title));
    }

    lines.push(String::new());
    lines.push("---".to_string());
    lines.push(
        "*Auto-generated by `hief patterns sync`. Edit individual pattern files, not this index.*"
            .to_string(),
    );

    std::fs::write(dir.join("INDEX.md"), lines.join("\n"))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn list_empty_when_dir_absent() {
        let root = tempdir().expect("tempdir should be created");
        assert!(list_patterns(root.path()).is_empty());
    }

    #[test]
    fn create_list_show_roundtrip() {
        let root = tempdir().expect("tempdir should be created");
        create_pattern(
            root.path(),
            "add-api-client",
            "# Pattern: Add API Client\n\n## Steps\n\n1. Create the file\n",
        )
        .expect("create pattern should succeed");
        let list = list_patterns(root.path());
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "add-api-client");
        let content =
            get_pattern(root.path(), "add-api-client").expect("pattern should be readable");
        assert!(content.contains("Add API Client"));
    }

    #[test]
    fn create_updates_index_md() {
        let root = tempdir().expect("tempdir should be created");
        create_pattern(
            root.path(),
            "my-pattern",
            "# Pattern: My Pattern\n\ncontent",
        )
        .expect("create pattern should succeed");
        let index = root.path().join(".hief").join("patterns").join("INDEX.md");
        assert!(index.exists());
        let text = std::fs::read_to_string(&index).expect("index should be readable");
        assert!(text.contains("my-pattern"));
    }

    #[test]
    fn index_excludes_index_md_itself() {
        let root = tempdir().expect("tempdir should be created");
        create_pattern(root.path(), "p1", "# Pattern: P1\ncontent").expect("create should succeed");
        create_pattern(root.path(), "p2", "# Pattern: P2\ncontent").expect("create should succeed");
        let list = list_patterns(root.path());
        assert!(
            list.iter().all(|p| p.name != "INDEX"),
            "INDEX must be excluded"
        );
        assert_eq!(list.len(), 2);
    }

    #[test]
    fn create_rejects_empty_content() {
        let root = tempdir().expect("tempdir should be created");
        assert!(create_pattern(root.path(), "empty", "   ").is_err());
    }

    #[test]
    fn create_rejects_path_traversal() {
        let root = tempdir().expect("tempdir should be created");
        assert!(create_pattern(root.path(), "../escape", "content").is_err());
    }

    #[test]
    fn get_missing_pattern_is_error() {
        let root = tempdir().expect("tempdir should be created");
        assert!(get_pattern(root.path(), "nosuch").is_err());
    }

    #[test]
    fn sync_index_on_empty_dir_is_noop() {
        let root = tempdir().expect("tempdir should be created");
        // Should not error even when patterns dir doesn't exist
        sync_index(root.path()).expect("sync on empty dir should succeed");
    }

    #[test]
    fn create_overwrites_existing_pattern() {
        let root = tempdir().expect("tempdir should be created");
        create_pattern(root.path(), "pat", "# Pattern: Pat\n\nv1")
            .expect("first create should succeed");
        create_pattern(root.path(), "pat", "# Pattern: Pat\n\nv2")
            .expect("second create should succeed");
        let content = get_pattern(root.path(), "pat").expect("pattern should be readable");
        assert!(content.contains("v2"));
        assert!(!content.contains("v1"));
    }
}
