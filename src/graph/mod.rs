//! Intent graph: SQL-based task dependency tracking.

pub mod edges;
pub mod intent;
pub mod query;

use serde::Serialize;

use crate::db::Database;
use crate::errors::{HiefError, Result};

pub use self::edges::IntentEdge;
pub use self::intent::Intent;

/// An intent with its dependency context.
#[derive(Debug, Clone, Serialize)]
pub struct IntentWithDeps {
    pub intent: Intent,
    pub depends_on: Vec<Intent>,
    pub blocks: Vec<Intent>,
    pub all_deps_satisfied: bool,
}

/// Valid intent kinds.
pub const VALID_KINDS: &[&str] = &["feature", "bug", "refactor", "spike", "test", "chore"];

/// Valid intent statuses.
pub const VALID_STATUSES: &[&str] = &[
    "draft",
    "approved",
    "in_progress",
    "in_review",
    "verified",
    "merged",
    "rejected",
    "blocked",
];

/// Valid intent priorities.
pub const VALID_PRIORITIES: &[&str] = &["critical", "high", "medium", "low"];

/// Valid edge kinds.
pub const VALID_EDGE_KINDS: &[&str] =
    &["depends_on", "blocks", "implements", "tests", "related_to"];

/// Validate that a status transition is allowed.
pub fn validate_transition(from: &str, to: &str) -> bool {
    matches!(
        (from, to),
        ("draft", "approved")
            | ("draft", "rejected")
            | ("approved", "in_progress")
            | ("approved", "rejected")
            | ("in_progress", "in_review")
            | ("in_progress", "rejected")
            | ("in_review", "verified")
            | ("in_review", "rejected")
            | ("in_review", "in_progress") // Revisions needed
            | ("verified", "merged")
            | ("blocked", "approved")
            | (_, "blocked") // Automatic
    )
}

/// Create a new intent.
pub async fn create_intent(db: &Database, intent: &Intent) -> Result<()> {
    intent::insert(db, intent).await
}

/// Get an intent by ID.
pub async fn get_intent(db: &Database, id: &str) -> Result<Intent> {
    intent::get(db, id).await
}

/// List intents with optional filters.
pub async fn list_intents(
    db: &Database,
    status: Option<&str>,
    kind: Option<&str>,
) -> Result<Vec<Intent>> {
    intent::list(db, status, kind).await
}

/// Update an intent's status with transition validation.
pub async fn update_status(db: &Database, id: &str, new_status: &str) -> Result<()> {
    let current = intent::get(db, id).await?;

    if !validate_transition(&current.status, new_status) {
        return Err(HiefError::InvalidTransition {
            from: current.status.clone(),
            to: new_status.to_string(),
        });
    }

    intent::update_status(db, id, new_status).await
}

/// Update an intent's assigned_to field.
pub async fn assign_intent(db: &Database, id: &str, assigned_to: &str) -> Result<()> {
    intent::assign(db, id, assigned_to).await
}

/// Add a dependency edge.
pub async fn add_edge(db: &Database, edge: &IntentEdge) -> Result<()> {
    edges::insert(db, edge).await
}

/// Get intents that are ready to work on (all dependencies satisfied).
pub async fn ready_intents(db: &Database) -> Result<Vec<Intent>> {
    query::ready_nodes(db).await
}

/// Get an intent with all its dependency context.
pub async fn get_intent_with_deps(db: &Database, id: &str) -> Result<IntentWithDeps> {
    let intent = intent::get(db, id).await?;
    let depends_on = query::get_dependencies(db, id).await?;
    let blocks = query::get_dependents(db, id).await?;
    let all_deps_satisfied = query::all_deps_satisfied(db, id).await?;

    Ok(IntentWithDeps {
        intent,
        depends_on,
        blocks,
        all_deps_satisfied,
    })
}

/// Validate graph integrity: check for cycles and orphans.
pub async fn validate_graph(db: &Database) -> Result<GraphValidation> {
    let cycles = query::detect_cycles(db).await?;
    let blocked = query::auto_block_rejected(db).await?;

    Ok(GraphValidation {
        has_cycles: !cycles.is_empty(),
        cycle_nodes: cycles,
        auto_blocked: blocked,
    })
}

/// Result of graph validation.
#[derive(Debug, Clone, Serialize)]
pub struct GraphValidation {
    pub has_cycles: bool,
    pub cycle_nodes: Vec<String>,
    pub auto_blocked: usize,
}
