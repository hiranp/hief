//! Golden set TOML loader.

use serde::Deserialize;
use std::path::Path;

use crate::errors::{HiefError, Result};

/// A golden evaluation set loaded from TOML.
#[derive(Debug, Clone, Deserialize)]
pub struct GoldenSet {
    pub metadata: GoldenMetadata,
    pub cases: Vec<EvalCase>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GoldenMetadata {
    pub name: String,
    #[allow(dead_code)]
    pub description: String,
}

/// A single evaluation case within a golden set.
#[derive(Debug, Clone, Deserialize)]
pub struct EvalCase {
    pub id: String,
    pub name: String,
    #[serde(default = "default_priority")]
    pub priority: String,
    #[allow(dead_code)]
    pub intent: Option<String>,
    pub checks: EvalChecks,
}

fn default_priority() -> String {
    "medium".to_string()
}

/// The checks to perform for an evaluation case.
#[derive(Debug, Clone, Deserialize)]
pub struct EvalChecks {
    #[serde(default)]
    pub must_contain: Vec<String>,
    #[serde(default)]
    pub must_not_contain: Vec<String>,
    /// Glob patterns to restrict which files are checked. Multiple patterns are
    /// combined as OR (a file matching any pattern is included). An empty list
    /// means all files are in scope.
    #[serde(default)]
    pub file_patterns: Vec<String>,
    /// Glob patterns to exclude from evaluation (useful for skipping tests or
    /// generated code). These are OR-combined: a file matching *any* exclude
    /// pattern will be ignored for this case.
    #[serde(default)]
    pub exclude_file_patterns: Vec<String>,
    /// Structural (ast-grep) patterns that MUST match somewhere in the codebase.
    /// Each entry has the format `"language:pattern"` (e.g. `"rust:pub fn $NAME($$$) -> Result<$RET, $ERR>"`).
    #[serde(default)]
    pub structural_must_contain: Vec<String>,
    /// Structural (ast-grep) patterns that MUST NOT match anywhere.
    /// Each entry has the format `"language:pattern"` (e.g. `"rust:$X.unwrap()"`).
    #[serde(default)]
    pub structural_must_not_contain: Vec<String>,
    /// If true, only evaluate files changed since the last eval run's git commit.
    /// Enables differential evaluation for faster, less noisy checks.
    #[serde(default)]
    pub diff_only: bool,
    /// Optional shell command to run as part of evaluation (e.g. "cargo test", "pytest").
    /// A non-zero exit code is recorded as a violation. The command runs in the
    /// project root. Omit to skip test-suite-based evaluation.
    #[serde(default)]
    pub test_command: Option<String>,
}

/// Load golden sets from the golden directory.
/// If `name` is provided, only load that specific set.
pub fn load_golden_sets(golden_dir: &Path, name: Option<&str>) -> Result<Vec<GoldenSet>> {
    if !golden_dir.exists() {
        return Ok(Vec::new());
    }

    let mut sets = Vec::new();

    let entries = std::fs::read_dir(golden_dir)?;

    for entry in entries {
        let entry = entry?;
        let path = entry.path();

        if path.extension().and_then(|e| e.to_str()) != Some("toml") {
            continue;
        }

        // If filtering by name, check the stem
        if let Some(filter_name) = name {
            let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
            if stem != filter_name {
                continue;
            }
        }

        let content = std::fs::read_to_string(&path)?;
        let set: GoldenSet = toml::from_str(&content)
            .map_err(|e| HiefError::GoldenSetParse(format!("{}: {}", path.display(), e)))?;
        sets.push(set);
    }

    if let Some(name) = name {
        if sets.is_empty() {
            return Err(HiefError::GoldenSetNotFound(name.to_string()));
        }
    }

    Ok(sets)
}

/// List available golden set names.
pub fn list_sets(golden_dir: &Path) -> Result<Vec<String>> {
    if !golden_dir.exists() {
        return Ok(Vec::new());
    }

    let mut names = Vec::new();
    let entries = std::fs::read_dir(golden_dir)?;

    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("toml") {
            if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                names.push(stem.to_string());
            }
        }
    }

    names.sort();
    Ok(names)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_golden_toml(dir: &Path, name: &str, content: &str) {
        let path = dir.join(format!("{}.toml", name));
        std::fs::write(&path, content).unwrap();
    }

    #[test]
    fn test_load_golden_set() {
        let dir = tempfile::tempdir().unwrap();
        write_golden_toml(
            dir.path(),
            "basic",
            r#"
[metadata]
name = "basic-checks"
description = "Basic code quality checks"

[[cases]]
id = "c1"
name = "Must have error handling"
priority = "critical"

[cases.checks]
must_contain = ["Result<", "Error"]
must_not_contain = ["unwrap()"]
file_patterns = ["*.rs"]
"#,
        );

        let sets = load_golden_sets(dir.path(), None).unwrap();
        assert_eq!(sets.len(), 1);
        assert_eq!(sets[0].metadata.name, "basic-checks");
        assert_eq!(sets[0].cases.len(), 1);
        assert_eq!(sets[0].cases[0].id, "c1");
        assert_eq!(sets[0].cases[0].priority, "critical");
        assert_eq!(
            sets[0].cases[0].checks.must_contain,
            vec!["Result<", "Error"]
        );
        assert_eq!(sets[0].cases[0].checks.must_not_contain, vec!["unwrap()"]);
    }

    #[test]
    fn test_load_golden_set_by_name() {
        let dir = tempfile::tempdir().unwrap();
        write_golden_toml(
            dir.path(),
            "set_a",
            r#"
[metadata]
name = "set-a"
description = "Set A"

[[cases]]
id = "a1"
name = "Check A"
[cases.checks]
must_contain = ["foo"]
"#,
        );
        write_golden_toml(
            dir.path(),
            "set_b",
            r#"
[metadata]
name = "set-b"
description = "Set B"

[[cases]]
id = "b1"
name = "Check B"
[cases.checks]
must_contain = ["bar"]
"#,
        );

        let sets = load_golden_sets(dir.path(), Some("set_a")).unwrap();
        assert_eq!(sets.len(), 1);
        assert_eq!(sets[0].metadata.name, "set-a");
    }

    #[test]
    fn test_load_nonexistent_named_set() {
        let dir = tempfile::tempdir().unwrap();
        let result = load_golden_sets(dir.path(), Some("nonexistent"));
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("golden set not found"), "got: {err}");
    }

    #[test]
    fn test_load_from_nonexistent_dir() {
        let sets = load_golden_sets(Path::new("/nonexistent/golden"), None).unwrap();
        assert!(sets.is_empty());
    }

    #[test]
    fn test_load_invalid_toml() {
        let dir = tempfile::tempdir().unwrap();
        write_golden_toml(dir.path(), "bad", "this is { not valid");

        let result = load_golden_sets(dir.path(), None);
        assert!(result.is_err());
    }

    #[test]
    fn test_list_sets() {
        let dir = tempfile::tempdir().unwrap();
        write_golden_toml(
            dir.path(),
            "alpha",
            "[metadata]\nname = \"a\"\ndescription = \"a\"\ncases = []",
        );
        write_golden_toml(
            dir.path(),
            "beta",
            "[metadata]\nname = \"b\"\ndescription = \"b\"\ncases = []",
        );

        let names = list_sets(dir.path()).unwrap();
        assert_eq!(names.len(), 2);
        assert!(names.contains(&"alpha".to_string()));
        assert!(names.contains(&"beta".to_string()));
    }

    #[test]
    fn test_list_sets_empty_dir() {
        let dir = tempfile::tempdir().unwrap();
        let names = list_sets(dir.path()).unwrap();
        assert!(names.is_empty());
    }

    #[test]
    fn test_list_sets_nonexistent_dir() {
        let names = list_sets(Path::new("/nonexistent")).unwrap();
        assert!(names.is_empty());
    }

    #[test]
    fn test_golden_set_default_priority() {
        let dir = tempfile::tempdir().unwrap();
        write_golden_toml(
            dir.path(),
            "defaults",
            r#"
[metadata]
name = "defaults"
description = "Test defaults"

[[cases]]
id = "d1"
name = "No priority set"
[cases.checks]
must_contain = ["something"]
"#,
        );

        let sets = load_golden_sets(dir.path(), None).unwrap();
        assert_eq!(sets[0].cases[0].priority, "medium"); // default
    }

    #[test]
    fn test_golden_set_with_intent() {
        let dir = tempfile::tempdir().unwrap();
        write_golden_toml(
            dir.path(),
            "with_intent",
            r#"
[metadata]
name = "intent-linked"
description = "Linked to intent"

[[cases]]
id = "i1"
name = "Intent linked check"
intent = "abc-123"
[cases.checks]
must_contain = ["foo"]
"#,
        );

        let sets = load_golden_sets(dir.path(), None).unwrap();
        assert_eq!(sets[0].cases[0].intent, Some("abc-123".to_string()));
    }
}
