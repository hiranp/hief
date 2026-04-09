//! Agent-maintained context layer (.hief/context/).
//!
//! Provides MCP-accessible read/write operations for persistent prose context
//! files: architecture, stack, conventions, decisions, setup, and any custom
//! files agents create during a session.
//!
//! Files are plain markdown. Agents read and update them after each task
//! (the GROW step). The drift checker monitors them for staleness.

use std::path::{Path, PathBuf};

use schemars::JsonSchema;
use serde::Serialize;

use crate::errors::{HiefError, Result};

const CONTEXT_DIR: &str = ".hief/context";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Metadata about a single context file.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct ContextFile {
    pub name: String,
    pub title: String,
    pub path: String,
    pub size_bytes: u64,
    pub last_modified: Option<i64>,
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn context_dir(project_root: &Path) -> PathBuf {
    project_root.join(CONTEXT_DIR)
}

/// Validate that a context file name is safe (no traversal, no absolute paths).
fn validate_name(name: &str) -> Result<()> {
    if name.is_empty() {
        return Err(HiefError::SecurityViolation(
            "context file name cannot be empty".to_string(),
        ));
    }
    if name.contains('/') || name.contains('\\') || name.contains("..") {
        return Err(HiefError::PathTraversal(format!(
            "context file name '{}' must be a bare filename, not a path",
            name
        )));
    }
    if name.starts_with('.') || name.starts_with('-') {
        return Err(HiefError::SecurityViolation(format!(
            "context file name '{}' cannot start with '.' or '-'",
            name
        )));
    }
    if !name
        .chars()
        .all(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == '.')
    {
        return Err(HiefError::SecurityViolation(format!(
            "context file name '{}' contains invalid characters",
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

fn to_display_name(filename: &str) -> String {
    filename.trim_end_matches(".md").to_string()
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

/// List all context files in .hief/context/.
pub fn list_context_files(project_root: &Path) -> Vec<ContextFile> {
    let dir = context_dir(project_root);
    if !dir.exists() {
        return Vec::new();
    }

    let mut files = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("md") {
                continue;
            }
            let filename = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            let name = to_display_name(&filename);
            let content = std::fs::read_to_string(&path).unwrap_or_default();
            let title = extract_title(&content, &name);

            let meta = path.metadata().ok();
            let size_bytes = meta.as_ref().map(|m| m.len()).unwrap_or(0);
            let last_modified = meta
                .as_ref()
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs() as i64);

            files.push(ContextFile {
                name,
                title,
                path: format!("{}/{}", CONTEXT_DIR, filename),
                size_bytes,
                last_modified,
            });
        }
    }

    files.sort_by(|a, b| a.name.cmp(&b.name));
    files
}

// ---------------------------------------------------------------------------
// Read
// ---------------------------------------------------------------------------

/// Read a context file by name (with or without .md extension).
pub fn read_context_file(project_root: &Path, name: &str) -> Result<String> {
    let bare = name.trim_end_matches(".md");
    validate_name(bare)?;
    let path = context_dir(project_root).join(to_filename(bare));
    if !path.exists() {
        return Err(HiefError::Other(format!(
            "context file '{}' not found — use list_context_files to see available files",
            name
        )));
    }
    Ok(std::fs::read_to_string(&path)?)
}

// ---------------------------------------------------------------------------
// Write
// ---------------------------------------------------------------------------

/// Write (create or overwrite) a context file.
///
/// Creates .hief/context/ if it doesn't exist.
/// Content is written as-is; agents are responsible for markdown formatting.
pub fn write_context_file(project_root: &Path, name: &str, content: &str) -> Result<()> {
    let bare = name.trim_end_matches(".md");
    validate_name(bare)?;

    if content.trim().is_empty() {
        return Err(HiefError::Other(
            "content is empty — provide meaningful content or use read_context_file to check the existing file".to_string(),
        ));
    }

    let dir = context_dir(project_root);
    std::fs::create_dir_all(&dir)?;
    std::fs::write(dir.join(to_filename(bare)), content)?;
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
    fn list_returns_empty_when_dir_absent() {
        let root = tempdir().expect("tempdir should be created");
        assert!(list_context_files(root.path()).is_empty());
    }

    #[test]
    fn write_read_roundtrip() {
        let root = tempdir().expect("tempdir should be created");
        write_context_file(root.path(), "architecture", "# Architecture\n\ncontent")
            .expect("write should succeed");
        let got = read_context_file(root.path(), "architecture").expect("read should succeed");
        assert!(got.contains("Architecture"));
        assert!(got.contains("content"));
    }

    #[test]
    fn write_with_md_extension_roundtrip() {
        let root = tempdir().expect("tempdir should be created");
        write_context_file(root.path(), "stack.md", "# Stack\n\nRust")
            .expect("write should succeed");
        let got = read_context_file(root.path(), "stack.md").expect("read should succeed");
        assert!(got.contains("Rust"));
    }

    #[test]
    fn list_shows_written_files() {
        let root = tempdir().expect("tempdir should be created");
        write_context_file(root.path(), "arch", "# Arch\n\nx").expect("write should succeed");
        write_context_file(root.path(), "decisions", "# Decisions\n\ny")
            .expect("write should succeed");
        let files = list_context_files(root.path());
        assert_eq!(files.len(), 2);
        let names: Vec<&str> = files.iter().map(|f| f.name.as_str()).collect();
        assert!(names.contains(&"arch"));
        assert!(names.contains(&"decisions"));
    }

    #[test]
    fn list_extracts_h1_as_title() {
        let root = tempdir().expect("tempdir should be created");
        write_context_file(root.path(), "arch", "# My Architecture\n\nx")
            .expect("write should succeed");
        let files = list_context_files(root.path());
        assert_eq!(files[0].title, "My Architecture");
    }

    #[test]
    fn read_missing_file_is_error() {
        let root = tempdir().expect("tempdir should be created");
        assert!(read_context_file(root.path(), "nope").is_err());
    }

    #[test]
    fn write_empty_content_is_error() {
        let root = tempdir().expect("tempdir should be created");
        assert!(write_context_file(root.path(), "test", "   ").is_err());
    }

    #[test]
    fn write_rejects_path_traversal() {
        let root = tempdir().expect("tempdir should be created");
        assert!(write_context_file(root.path(), "../escape", "content").is_err());
    }

    #[test]
    fn write_rejects_slash_in_name() {
        let root = tempdir().expect("tempdir should be created");
        assert!(write_context_file(root.path(), "a/b", "content").is_err());
    }

    #[test]
    fn overwrite_updates_content() {
        let root = tempdir().expect("tempdir should be created");
        write_context_file(root.path(), "doc", "# Doc\n\nv1").expect("write should succeed");
        write_context_file(root.path(), "doc", "# Doc\n\nv2").expect("write should succeed");
        let got = read_context_file(root.path(), "doc").expect("read should succeed");
        assert!(got.contains("v2"));
        assert!(!got.contains("v1"));
    }
}
