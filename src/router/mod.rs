//! Session routing table (.hief/router.toml).
//!
//! Maps task types to the context files and patterns the agent should load.
//! This keeps cold-start token cost low — the agent loads only what's relevant
//! for the current task rather than dumping all context into its window.
//!
//! # File format (.hief/router.toml)
//!
//! ```toml
//! [[routes]]
//! task_kinds = ["architecture", "understand"]
//! description = "Understand how components connect"
//! load = ["context/architecture.md", "context/decisions.md"]
//!
//! [[routes]]
//! task_kinds = ["debug", "error", "fix"]
//! description = "Debug a problem"
//! load = ["context/architecture.md"]
//! load_patterns = ["debug-*"]
//! ```

use std::path::{Path, PathBuf};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::errors::{HiefError, Result};

const ROUTER_FILE: &str = ".hief/router.toml";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// A single routing rule mapping task keywords to context files.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Route {
    /// Keywords that identify this type of task (e.g. ["debug", "error", "fix"]).
    pub task_kinds: Vec<String>,
    /// Human-readable description of when to use this route.
    #[serde(default)]
    pub description: String,
    /// Context files to load, relative to .hief/ (e.g. ["context/architecture.md"]).
    #[serde(default)]
    pub load: Vec<String>,
    /// Pattern name globs — loads matching .hief/patterns/*.md files (e.g. ["debug-*"]).
    #[serde(default)]
    pub load_patterns: Vec<String>,
}

/// The full routing table returned by `get_routing_table`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RoutingTable {
    pub routes: Vec<Route>,
    /// True if a custom .hief/router.toml was found; false if defaults are used.
    pub loaded: bool,
    /// Relative path to the router file.
    pub path: String,
}

// ---------------------------------------------------------------------------
// Load
// ---------------------------------------------------------------------------

/// Load the routing table from .hief/router.toml.
///
/// Returns built-in defaults with `loaded = false` if the file doesn't exist.
pub fn load_routing_table(project_root: &Path) -> Result<RoutingTable> {
    let path = router_path(project_root);

    if !path.exists() {
        return Ok(RoutingTable {
            routes: default_routes(),
            loaded: false,
            path: ROUTER_FILE.to_string(),
        });
    }

    let content = std::fs::read_to_string(&path)?;

    #[derive(Deserialize)]
    struct TomlTable {
        #[serde(default)]
        routes: Vec<Route>,
    }

    let table: TomlTable = toml::from_str(&content)
        .map_err(|e| HiefError::Config(format!("failed to parse {}: {}", ROUTER_FILE, e)))?;

    Ok(RoutingTable {
        routes: table.routes,
        loaded: true,
        path: ROUTER_FILE.to_string(),
    })
}

/// Write the default routing table scaffold to .hief/router.toml.
pub fn write_default(project_root: &Path) -> Result<()> {
    let path = router_path(project_root);
    std::fs::write(&path, DEFAULT_ROUTER_TOML)?;
    Ok(())
}

fn router_path(project_root: &Path) -> PathBuf {
    project_root.join(ROUTER_FILE)
}

// ---------------------------------------------------------------------------
// Defaults
// ---------------------------------------------------------------------------

fn default_routes() -> Vec<Route> {
    vec![
        Route {
            task_kinds: vec![
                "architecture".into(),
                "understand".into(),
                "overview".into(),
            ],
            description: "Understand how components connect".into(),
            load: vec!["context/architecture.md".into(), "context/stack.md".into()],
            load_patterns: vec![],
        },
        Route {
            task_kinds: vec!["debug".into(), "error".into(), "fix".into(), "bug".into()],
            description: "Debug a problem or fix a bug".into(),
            load: vec!["context/architecture.md".into()],
            load_patterns: vec!["debug-*".into()],
        },
        Route {
            task_kinds: vec![
                "feature".into(),
                "implement".into(),
                "add".into(),
                "create".into(),
            ],
            description: "Add a new feature".into(),
            load: vec![
                "context/conventions.md".into(),
                "context/architecture.md".into(),
            ],
            load_patterns: vec![],
        },
        Route {
            task_kinds: vec![
                "convention".into(),
                "style".into(),
                "review".into(),
                "lint".into(),
            ],
            description: "Check or enforce code conventions".into(),
            load: vec!["context/conventions.md".into()],
            load_patterns: vec![],
        },
        Route {
            task_kinds: vec!["setup".into(), "install".into(), "run".into(), "dev".into()],
            description: "Set up or run the project locally".into(),
            load: vec!["context/setup.md".into()],
            load_patterns: vec![],
        },
        Route {
            task_kinds: vec![
                "decision".into(),
                "design".into(),
                "tradeoff".into(),
                "architecture".into(),
            ],
            description: "Make or review an architectural decision".into(),
            load: vec![
                "context/decisions.md".into(),
                "context/architecture.md".into(),
            ],
            load_patterns: vec![],
        },
    ]
}

const DEFAULT_ROUTER_TOML: &str = r#"# HIEF Session Router
#
# Maps task keywords to context files and patterns the agent should load.
# Call `get_routing_table` at the start of each session, then load the
# context files matching the current task to reduce cold-start token cost.
#
# Format:
#   [[routes]]
#   task_kinds = ["keyword1", "keyword2"]  -- trigger words for this task type
#   description = "..."                    -- when to use this route
#   load = ["context/architecture.md"]    -- .hief/ files to load
#   load_patterns = ["debug-*"]            -- .hief/patterns/*.md globs

[[routes]]
task_kinds = ["architecture", "understand", "overview"]
description = "Understand how components connect"
load = ["context/architecture.md", "context/stack.md"]

[[routes]]
task_kinds = ["debug", "error", "fix", "bug"]
description = "Debug a problem or fix a bug"
load = ["context/architecture.md"]
load_patterns = ["debug-*"]

[[routes]]
task_kinds = ["feature", "implement", "add", "create"]
description = "Add a new feature"
load = ["context/conventions.md", "context/architecture.md"]

[[routes]]
task_kinds = ["convention", "style", "review", "lint"]
description = "Check or enforce code conventions"
load = ["context/conventions.md"]

[[routes]]
task_kinds = ["setup", "install", "run", "dev"]
description = "Set up or run the project locally"
load = ["context/setup.md"]

[[routes]]
task_kinds = ["decision", "design", "tradeoff"]
description = "Make or review an architectural decision"
load = ["context/decisions.md", "context/architecture.md"]
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn returns_defaults_when_file_absent() {
        let root = tempdir().unwrap();
        let table = load_routing_table(root.path()).unwrap();
        assert!(!table.loaded);
        assert!(!table.routes.is_empty());
    }

    #[test]
    fn write_default_then_load() {
        let root = tempdir().unwrap();
        std::fs::create_dir_all(root.path().join(".hief")).unwrap();
        write_default(root.path()).unwrap();
        let table = load_routing_table(root.path()).unwrap();
        assert!(table.loaded);
        assert!(!table.routes.is_empty());
    }

    #[test]
    fn defaults_cover_debug_and_architecture() {
        let root = tempdir().unwrap();
        let table = load_routing_table(root.path()).unwrap();
        assert!(
            table
                .routes
                .iter()
                .any(|r| r.task_kinds.iter().any(|k| k == "debug")),
            "no debug route"
        );
        assert!(
            table
                .routes
                .iter()
                .any(|r| r.task_kinds.iter().any(|k| k == "architecture")),
            "no architecture route"
        );
    }

    #[test]
    fn custom_toml_overrides_defaults() {
        let root = tempdir().unwrap();
        let hief_dir = root.path().join(".hief");
        std::fs::create_dir_all(&hief_dir).unwrap();
        std::fs::write(
            hief_dir.join("router.toml"),
            "[[routes]]\ntask_kinds = [\"custom\"]\ndescription = \"test\"\n",
        )
        .unwrap();
        let table = load_routing_table(root.path()).unwrap();
        assert!(table.loaded);
        assert_eq!(table.routes.len(), 1);
        assert_eq!(table.routes[0].task_kinds[0], "custom");
    }

    #[test]
    fn invalid_toml_returns_error() {
        let root = tempdir().unwrap();
        let hief_dir = root.path().join(".hief");
        std::fs::create_dir_all(&hief_dir).unwrap();
        std::fs::write(hief_dir.join("router.toml"), "not valid toml {{{{").unwrap();
        assert!(load_routing_table(root.path()).is_err());
    }

    #[test]
    fn route_fields_have_defaults() {
        let root = tempdir().unwrap();
        let hief_dir = root.path().join(".hief");
        std::fs::create_dir_all(&hief_dir).unwrap();
        // Minimal route with only required task_kinds
        std::fs::write(
            hief_dir.join("router.toml"),
            "[[routes]]\ntask_kinds = [\"test\"]\n",
        )
        .unwrap();
        let table = load_routing_table(root.path()).unwrap();
        assert_eq!(table.routes[0].load.len(), 0);
        assert_eq!(table.routes[0].load_patterns.len(), 0);
        assert!(table.routes[0].description.is_empty());
    }
}
