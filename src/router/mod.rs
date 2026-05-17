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

use crate::db::{Database, RetrievalWeightSnapshotWrite};
use crate::errors::{HiefError, Result};

const ROUTER_FILE: &str = ".hief/router.toml";

/// Retrieval strategy chosen for code search queries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum RetrievalStrategy {
    /// Fast lexical-only path for exact symbol lookups.
    Deterministic { top_k: usize },
    /// Hybrid lexical + semantic path for mixed or ambiguous queries.
    Hybrid {
        lexical_k: usize,
        semantic_k: usize,
        rrf_k: u32,
    },
    /// Full semantic path for conceptual queries.
    Semantic { top_k: usize, rerank: bool },
}

/// Policy lane chosen for a protocol operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum ProtocolLane {
    /// Execute locally in the CLI lane.
    Cli,
    /// Execute as a single MCP operation.
    Mcp,
    /// Execute through a progressive MCP flow under token pressure.
    ProgressiveMcp,
}

impl ProtocolLane {
    /// Returns the stable string form used in config and user-facing explanations.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Cli => "cli",
            Self::Mcp => "mcp",
            Self::ProgressiveMcp => "progressive-mcp",
        }
    }
}

/// Minimal operation metadata needed for deterministic protocol lane routing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct OperationRequest {
    /// Stable operation name used in diagnostics and tests.
    pub operation: String,
    /// True when the operation is local and deterministic.
    pub local_deterministic: bool,
    /// True when the operation requires a remote or authenticated MCP path.
    pub remote_auth_required: bool,
    /// Estimated response or context size used for token-pressure routing.
    pub estimated_tokens: usize,
}

impl OperationRequest {
    /// Creates a local deterministic operation.
    #[allow(dead_code)]
    pub fn local(operation: impl Into<String>, estimated_tokens: usize) -> Self {
        Self {
            operation: operation.into(),
            local_deterministic: true,
            remote_auth_required: false,
            estimated_tokens,
        }
    }

    /// Creates a remote or auth-bound operation.
    pub fn remote(operation: impl Into<String>, estimated_tokens: usize) -> Self {
        Self {
            operation: operation.into(),
            local_deterministic: false,
            remote_auth_required: true,
            estimated_tokens,
        }
    }
}

/// Deterministic explanation for a lane-routing decision.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct LaneDecision {
    pub lane: ProtocolLane,
    pub reason: String,
}

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
// Retrieval routing
// ---------------------------------------------------------------------------

/// Route a search query to a retrieval strategy based on query shape.
pub fn route_query(query: &str) -> RetrievalStrategy {
    let normalized = query.trim();

    if normalized.is_empty() {
        return default_hybrid();
    }

    let token_count = normalized.split_whitespace().count();
    let has_symbol_markers = has_symbol_markers(normalized);
    let exact_identifier = is_exact_identifier_query(normalized, token_count);
    let conceptual = is_conceptual_query(normalized, token_count);

    match (exact_identifier || has_symbol_markers, conceptual) {
        (true, true) => default_hybrid(),
        (true, false) => RetrievalStrategy::Deterministic { top_k: 10 },
        (false, true) => RetrievalStrategy::Semantic {
            top_k: 15,
            rerank: true,
        },
        (false, false) => default_hybrid(),
    }
}

/// Selects the protocol lane for an operation using deterministic policy rules.
pub fn select_lane(
    request: &OperationRequest,
    config: &crate::config::RouterConfig,
) -> LaneDecision {
    if request.estimated_tokens >= config.token_pressure_threshold {
        return LaneDecision {
            lane: ProtocolLane::ProgressiveMcp,
            reason: format!(
                "estimated token pressure {} exceeds threshold {}",
                request.estimated_tokens, config.token_pressure_threshold
            ),
        };
    }

    if request.remote_auth_required {
        return LaneDecision {
            lane: ProtocolLane::Mcp,
            reason: "operation requires remote or authenticated execution".to_string(),
        };
    }

    if request.local_deterministic {
        return LaneDecision {
            lane: ProtocolLane::Cli,
            reason: "operation is local and deterministic".to_string(),
        };
    }

    LaneDecision {
        lane: config.default_lane,
        reason: format!(
            "default lane policy selected {}",
            config.default_lane.as_str()
        ),
    }
}

fn default_hybrid() -> RetrievalStrategy {
    RetrievalStrategy::Hybrid {
        lexical_k: 10,
        semantic_k: 10,
        rrf_k: 60,
    }
}

fn has_symbol_markers(query: &str) -> bool {
    query.contains("::") || query.contains("->") || query.contains('.') || query.contains('#')
}

fn is_exact_identifier_query(query: &str, token_count: usize) -> bool {
    if token_count == 1 {
        return looks_like_identifier(query);
    }

    token_count <= 3 && query.split_whitespace().all(looks_like_identifier)
}

fn looks_like_identifier(token: &str) -> bool {
    let token = token.trim_matches(|ch: char| !ch.is_alphanumeric() && ch != '_' && ch != ':');

    if token.is_empty() {
        return false;
    }

    let has_identifier_separators = token.contains('_') || token.contains("::");
    let has_camel_case = token.chars().any(|ch| ch.is_uppercase())
        && token.chars().any(|ch| ch.is_lowercase());
    let has_digits = token.chars().any(|ch| ch.is_ascii_digit());
    let short_enough = token.len() <= 32;

    short_enough && (has_identifier_separators || has_camel_case || has_digits)
}

fn is_conceptual_query(query: &str, token_count: usize) -> bool {
    let lowered = query.to_ascii_lowercase();

    if [
        "how ",
        "why ",
        "what ",
        "when ",
        "where ",
        "which ",
        "who ",
        "explain ",
        "describe ",
        "compare ",
    ]
    .iter()
    .any(|prefix| lowered.starts_with(prefix))
    {
        return true;
    }

    token_count >= 5
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

const RETRIEVAL_WEIGHT_STEP_MAX: f64 = 0.05;
const RETRIEVAL_LEARNING_MIN_SAMPLES: i64 = 5;

/// Retrieval blending weights used by lexical and semantic lanes.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RetrievalWeights {
    pub lexical: f64,
    pub semantic: f64,
    pub co_access: f64,
    pub recency: f64,
}

impl Default for RetrievalWeights {
    fn default() -> Self {
        Self {
            lexical: 0.4,
            semantic: 0.4,
            co_access: 0.1,
            recency: 0.1,
        }
    }
}

impl RetrievalWeights {
    pub fn lane_weight(self, lane: &str) -> f64 {
        match lane {
            "lexical" => self.lexical,
            "semantic" => self.semantic,
            "co_access" => self.co_access,
            "recency" => self.recency,
            _ => 1.0,
        }
    }

    fn bounded_adjust(self, lexical_delta: f64, semantic_delta: f64) -> Self {
        let lexical = (self.lexical + lexical_delta).clamp(0.05, 0.8);
        let semantic = (self.semantic + semantic_delta).clamp(0.05, 0.8);
        let mut adjusted = Self {
            lexical,
            semantic,
            co_access: self.co_access.clamp(0.05, 0.6),
            recency: self.recency.clamp(0.05, 0.6),
        };
        adjusted.normalize();
        adjusted
    }

    fn normalize(&mut self) {
        let sum = self.lexical + self.semantic + self.co_access + self.recency;
        if sum <= f64::EPSILON {
            *self = Self::default();
            return;
        }
        self.lexical /= sum;
        self.semantic /= sum;
        self.co_access /= sum;
        self.recency /= sum;
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RetrievalLearningStatus {
    pub learning_state: String,
    pub last_learning_outcome: String,
    pub candidate_delta: f64,
}

/// Get the latest active retrieval weights.
pub async fn active_retrieval_weights(db: &Database) -> Result<RetrievalWeights> {
    let latest = db.latest_retrieval_weight_snapshot().await?;
    if let Some(snapshot) = latest {
        serde_json::from_str::<RetrievalWeights>(&snapshot.current_json)
            .map_err(|e| HiefError::Other(format!("invalid retrieval weight snapshot: {e}")))
    } else {
        Ok(RetrievalWeights::default())
    }
}

/// Learn bounded retrieval weight candidates from recent groundedness telemetry.
pub async fn learn_retrieval_weights(
    db: &Database,
    wave_gate_open: bool,
) -> Result<RetrievalLearningStatus> {
    let current = active_retrieval_weights(db).await?;
    let (sample_size, avg_groundedness) = db.recent_groundedness_window(100).await?;

    if sample_size < RETRIEVAL_LEARNING_MIN_SAMPLES || avg_groundedness.is_none() {
        db.insert_retrieval_weight_snapshot(&RetrievalWeightSnapshotWrite {
            current_json: serde_json::to_string(&current)
                .map_err(|e| HiefError::Other(e.to_string()))?,
            candidate_json: None,
            applied: false,
            sample_size,
            outcome_label: "neutral".to_string(),
            candidate_delta: Some(0.0),
            learning_state: "neutral".to_string(),
            last_learning_outcome: Some("no_history".to_string()),
        })
        .await?;

        return Ok(RetrievalLearningStatus {
            learning_state: "neutral".to_string(),
            last_learning_outcome: "no_history".to_string(),
            candidate_delta: 0.0,
        });
    }

    let avg = avg_groundedness.unwrap_or(0.5);
    let shift = ((avg - 0.5) * 0.2).clamp(-RETRIEVAL_WEIGHT_STEP_MAX, RETRIEVAL_WEIGHT_STEP_MAX);
    let candidate = current.bounded_adjust(-shift, shift);
    let candidate_delta = ((candidate.lexical - current.lexical).abs()
        + (candidate.semantic - current.semantic).abs()
        + (candidate.co_access - current.co_access).abs()
        + (candidate.recency - current.recency).abs())
        / 4.0;

    let outcome_label = if shift > 0.005 {
        "improving"
    } else if shift < -0.005 {
        "regressing"
    } else {
        "neutral"
    };

    let apply_candidate = wave_gate_open && outcome_label == "improving";
    let new_current = if apply_candidate { candidate } else { current };
    let learning_state = if outcome_label == "regressing" {
        "regressing"
    } else if apply_candidate {
        "improving"
    } else {
        "neutral"
    };
    let last_learning_outcome = if apply_candidate {
        "promoted"
    } else if outcome_label == "regressing" {
        "rolled_back"
    } else {
        "no_change"
    };

    db.insert_retrieval_weight_snapshot(&RetrievalWeightSnapshotWrite {
        current_json: serde_json::to_string(&new_current)
            .map_err(|e| HiefError::Other(e.to_string()))?,
        candidate_json: Some(
            serde_json::to_string(&candidate).map_err(|e| HiefError::Other(e.to_string()))?,
        ),
        applied: apply_candidate,
        sample_size,
        outcome_label: outcome_label.to_string(),
        candidate_delta: Some(candidate_delta),
        learning_state: learning_state.to_string(),
        last_learning_outcome: Some(last_learning_outcome.to_string()),
    })
    .await?;

    Ok(RetrievalLearningStatus {
        learning_state: learning_state.to_string(),
        last_learning_outcome: last_learning_outcome.to_string(),
        candidate_delta,
    })
}

/// Emit shadow baseline-vs-candidate scoring telemetry without changing public schemas.
pub async fn emit_shadow_signal(
    db: &Database,
    lane: &str,
    quality_signal: Option<f64>,
) -> Result<()> {
    let Some(snapshot) = db.latest_retrieval_weight_snapshot().await? else {
        return Ok(());
    };
    let Some(candidate_json) = snapshot.candidate_json else {
        return Ok(());
    };

    let baseline = serde_json::from_str::<RetrievalWeights>(&snapshot.current_json)
        .map_err(|e| HiefError::Other(format!("invalid baseline weights: {e}")))?;
    let candidate = serde_json::from_str::<RetrievalWeights>(&candidate_json)
        .map_err(|e| HiefError::Other(format!("invalid candidate weights: {e}")))?;

    let quality = quality_signal.unwrap_or(0.0);
    let baseline_score = quality * baseline.lane_weight(lane);
    let candidate_score = quality * candidate.lane_weight(lane);

    let strategy = format!(
        "lane={lane};baseline={baseline_score:.4};candidate={candidate_score:.4};mode=shadow"
    );
    let _ = db
        .record_tool_event_scoped(
            "retrieval-learning-shadow",
            "retrieval_shadow",
            lane,
            Some(&strategy),
            Some(1),
            Some(0),
            quality_signal,
            Some("project-root"),
        )
        .await;

    Ok(())
}

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

    #[test]
    fn route_query_symbol_queries_are_deterministic() {
        assert_eq!(
            route_query("src::router::route_query"),
            RetrievalStrategy::Deterministic { top_k: 10 }
        );
    }

    #[test]
    fn route_query_conceptual_queries_are_semantic() {
        assert_eq!(
            route_query("how does adaptive retrieval routing work"),
            RetrievalStrategy::Semantic {
                top_k: 15,
                rerank: true,
            }
        );
    }

    #[test]
    fn route_query_mixed_queries_are_hybrid() {
        assert_eq!(
            route_query("how does src::router::route_query work"),
            RetrievalStrategy::Hybrid {
                lexical_k: 10,
                semantic_k: 10,
                rrf_k: 60,
            }
        );
    }

    #[test]
    fn route_query_default_fallback_is_hybrid() {
        assert_eq!(
            route_query("   "),
            RetrievalStrategy::Hybrid {
                lexical_k: 10,
                semantic_k: 10,
                rrf_k: 60,
            }
        );
    }
}
