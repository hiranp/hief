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
    pub description: String,
}

/// A single evaluation case within a golden set.
#[derive(Debug, Clone, Deserialize)]
pub struct EvalCase {
    pub id: String,
    pub name: String,
    #[serde(default = "default_priority")]
    pub priority: String,
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
    #[serde(default)]
    pub file_patterns: Option<Vec<String>>,
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
            let stem = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("");
            if stem != filter_name {
                continue;
            }
        }

        let content = std::fs::read_to_string(&path)?;
        let set: GoldenSet = toml::from_str(&content).map_err(|e| {
            HiefError::GoldenSetParse(format!("{}: {}", path.display(), e))
        })?;
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
