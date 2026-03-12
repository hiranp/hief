//! Intent graph: SQL-based task dependency tracking.

pub mod edges;
pub mod intent;
pub mod query;

use serde::Serialize;

use crate::db::Database;
use crate::errors::{HiefError, Result};

pub use self::edges::IntentEdge;
pub use self::intent::Intent;
pub use self::intent::resolve_id;

/// An intent with its dependency context.
#[derive(Debug, Clone, Serialize)]
pub struct IntentWithDeps {
    pub intent: Intent,
    pub depends_on: Vec<Intent>,
    pub blocks: Vec<Intent>,
    pub all_deps_satisfied: bool,
}

/// Valid intent kinds.
#[allow(dead_code)]
pub const VALID_KINDS: &[&str] = &["feature", "bug", "refactor", "spike", "test", "chore"];

/// Valid intent statuses.
#[allow(dead_code)]
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
#[allow(dead_code)]
pub const VALID_PRIORITIES: &[&str] = &["critical", "high", "medium", "low"];

/// Valid edge kinds.
#[allow(dead_code)]
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

/// Recover intents stuck in `in_progress` beyond a configurable stale timeout.
///
/// An agent may crash, time out, or be killed while holding an intent in
/// `in_progress`. Without recovery, downstream intents wait forever. This
/// function resets stale intents to `approved` so the next agent can pick
/// them up. It intentionally bypasses the normal state-machine (which does
/// not permit `in_progress → approved`) because stale recovery is an
/// administrative operation, not a workflow transition.
///
/// Returns the number of intents recovered. Set `timeout_hours = 0` to disable.
pub async fn recover_stale_intents(db: &Database, timeout_hours: u64) -> Result<usize> {
    if timeout_hours == 0 {
        return Ok(0);
    }
    let timeout_secs = timeout_hours as i64 * 3600;
    let affected = db
        .conn()
        .execute(
            "UPDATE intents SET status = 'approved', updated_at = unixepoch()
             WHERE status = 'in_progress'
               AND (unixepoch() - updated_at) > ?1",
            [timeout_secs],
        )
        .await
        .map_err(HiefError::Database)?;
    Ok(affected as usize)
}

/// Get all transitive dependencies of an intent (recursive).
pub async fn transitive_deps(db: &Database, id: &str) -> Result<Vec<Intent>> {
    query::transitive_deps(db, id).await
}

/// Result of graph validation.
#[derive(Debug, Clone, Serialize)]
pub struct GraphValidation {
    pub has_cycles: bool,
    pub cycle_nodes: Vec<String>,
    pub auto_blocked: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // Status transition validation
    // -----------------------------------------------------------------------

    #[test]
    fn test_valid_forward_transitions() {
        assert!(validate_transition("draft", "approved"));
        assert!(validate_transition("approved", "in_progress"));
        assert!(validate_transition("in_progress", "in_review"));
        assert!(validate_transition("in_review", "verified"));
        assert!(validate_transition("verified", "merged"));
    }

    #[test]
    fn test_valid_rejection_transitions() {
        assert!(validate_transition("draft", "rejected"));
        assert!(validate_transition("approved", "rejected"));
        assert!(validate_transition("in_progress", "rejected"));
        assert!(validate_transition("in_review", "rejected"));
    }

    #[test]
    fn test_valid_revision_transition() {
        assert!(validate_transition("in_review", "in_progress"));
    }

    #[test]
    fn test_valid_unblock_transition() {
        assert!(validate_transition("blocked", "approved"));
    }

    #[test]
    fn test_any_to_blocked() {
        assert!(validate_transition("draft", "blocked"));
        assert!(validate_transition("approved", "blocked"));
        assert!(validate_transition("in_progress", "blocked"));
        assert!(validate_transition("in_review", "blocked"));
    }

    #[test]
    fn test_invalid_transitions() {
        assert!(!validate_transition("draft", "in_progress")); // must go through approved
        assert!(!validate_transition("draft", "merged"));
        assert!(!validate_transition("merged", "draft"));
        assert!(!validate_transition("rejected", "approved"));
        assert!(!validate_transition("in_progress", "approved"));
        assert!(!validate_transition("verified", "draft"));
        assert!(!validate_transition("draft", "verified"));
        assert!(!validate_transition("approved", "merged"));
    }

    #[test]
    fn test_same_status_transition_invalid() {
        assert!(!validate_transition("draft", "draft"));
        assert!(!validate_transition("approved", "approved"));
    }

    // -----------------------------------------------------------------------
    // Constants validation
    // -----------------------------------------------------------------------

    #[test]
    fn test_valid_kinds_contains_expected() {
        assert!(VALID_KINDS.contains(&"feature"));
        assert!(VALID_KINDS.contains(&"bug"));
        assert!(VALID_KINDS.contains(&"refactor"));
        assert!(VALID_KINDS.contains(&"chore"));
        assert!(!VALID_KINDS.contains(&"invalid"));
    }

    #[test]
    fn test_valid_statuses_contains_expected() {
        assert!(VALID_STATUSES.contains(&"draft"));
        assert!(VALID_STATUSES.contains(&"approved"));
        assert!(VALID_STATUSES.contains(&"merged"));
        assert!(!VALID_STATUSES.contains(&"invalid"));
    }

    #[test]
    fn test_valid_priorities_contains_expected() {
        assert!(VALID_PRIORITIES.contains(&"critical"));
        assert!(VALID_PRIORITIES.contains(&"high"));
        assert!(VALID_PRIORITIES.contains(&"medium"));
        assert!(VALID_PRIORITIES.contains(&"low"));
        assert!(!VALID_PRIORITIES.contains(&"urgent"));
    }

    #[test]
    fn test_valid_edge_kinds() {
        assert!(VALID_EDGE_KINDS.contains(&"depends_on"));
        assert!(VALID_EDGE_KINDS.contains(&"blocks"));
        assert!(VALID_EDGE_KINDS.contains(&"implements"));
        assert!(VALID_EDGE_KINDS.contains(&"tests"));
        assert!(VALID_EDGE_KINDS.contains(&"related_to"));
    }

    // -----------------------------------------------------------------------
    // Database integration tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_create_and_get_intent() {
        let db = crate::db::Database::open_memory()
            .await
            .expect("db operation failed");
        let intent = Intent::new("feature", "Test intent", None, None);
        create_intent(&db, &intent)
            .await
            .expect("db operation failed");

        let fetched = get_intent(&db, &intent.id)
            .await
            .expect("db operation failed");
        assert_eq!(fetched.id, intent.id);
        assert_eq!(fetched.title, "Test intent");
        assert_eq!(fetched.kind, "feature");
        assert_eq!(fetched.status, "draft");
        assert_eq!(fetched.priority, "medium");
    }

    #[tokio::test]
    async fn test_get_nonexistent_intent() {
        let db = crate::db::Database::open_memory()
            .await
            .expect("db operation failed");
        let result = get_intent(&db, "nonexistent-uuid").await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("intent not found"), "got: {err}");
    }

    #[tokio::test]
    async fn test_list_intents_empty() {
        let db = crate::db::Database::open_memory()
            .await
            .expect("db operation failed");
        let intents = list_intents(&db, None, None)
            .await
            .expect("db operation failed");
        assert!(intents.is_empty());
    }

    #[tokio::test]
    async fn test_list_intents_with_filter() {
        let db = crate::db::Database::open_memory()
            .await
            .expect("db operation failed");

        let i1 = Intent::new("feature", "Feature 1", None, None);
        let i2 = Intent::new("bug", "Bug 1", None, None);
        create_intent(&db, &i1).await.expect("db operation failed");
        create_intent(&db, &i2).await.expect("db operation failed");

        let all = list_intents(&db, None, None)
            .await
            .expect("db operation failed");
        assert_eq!(all.len(), 2);

        let bugs = list_intents(&db, None, Some("bug"))
            .await
            .expect("db operation failed");
        assert_eq!(bugs.len(), 1);
        assert_eq!(bugs[0].kind, "bug");

        let drafts = list_intents(&db, Some("draft"), None)
            .await
            .expect("db operation failed");
        assert_eq!(drafts.len(), 2);
    }

    #[tokio::test]
    async fn test_status_update_valid() {
        let db = crate::db::Database::open_memory()
            .await
            .expect("db operation failed");
        let intent = Intent::new("feature", "Test", None, None);
        create_intent(&db, &intent)
            .await
            .expect("db operation failed");

        // draft -> approved
        update_status(&db, &intent.id, "approved")
            .await
            .expect("db operation failed");
        let fetched = get_intent(&db, &intent.id)
            .await
            .expect("db operation failed");
        assert_eq!(fetched.status, "approved");

        // approved -> in_progress
        update_status(&db, &intent.id, "in_progress")
            .await
            .expect("db operation failed");
        let fetched = get_intent(&db, &intent.id)
            .await
            .expect("db operation failed");
        assert_eq!(fetched.status, "in_progress");
    }

    #[tokio::test]
    async fn test_status_update_invalid_transition() {
        let db = crate::db::Database::open_memory()
            .await
            .expect("db operation failed");
        let intent = Intent::new("feature", "Test", None, None);
        create_intent(&db, &intent)
            .await
            .expect("db operation failed");

        // draft -> merged (invalid, must go through approved/in_progress/etc.)
        let result = update_status(&db, &intent.id, "merged").await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("invalid status transition"), "got: {err}");
    }

    #[tokio::test]
    async fn test_assign_intent() {
        let db = crate::db::Database::open_memory()
            .await
            .expect("db operation failed");
        let intent = Intent::new("feature", "Test", None, None);
        create_intent(&db, &intent)
            .await
            .expect("db operation failed");

        assign_intent(&db, &intent.id, "agent-claude")
            .await
            .expect("db operation failed");
        let fetched = get_intent(&db, &intent.id)
            .await
            .expect("db operation failed");
        assert_eq!(fetched.assigned_to, Some("agent-claude".to_string()));
    }

    #[tokio::test]
    async fn test_add_edge_and_ready_intents() {
        let db = crate::db::Database::open_memory()
            .await
            .expect("db operation failed");

        let i1 = Intent::new("feature", "Base feature", None, None);
        let i2 = Intent::new("feature", "Depends on base", None, None);
        create_intent(&db, &i1).await.expect("db operation failed");
        create_intent(&db, &i2).await.expect("db operation failed");

        // i2 depends on i1
        let edge = IntentEdge::depends_on(&i2.id, &i1.id);
        add_edge(&db, &edge).await.expect("db operation failed");

        // Approve both
        update_status(&db, &i1.id, "approved")
            .await
            .expect("db operation failed");
        update_status(&db, &i2.id, "approved")
            .await
            .expect("db operation failed");

        // i2 should NOT be ready (i1 is only approved, not verified/merged)
        let ready = ready_intents(&db).await.expect("db operation failed");
        let ready_ids: Vec<&str> = ready.iter().map(|i| i.id.as_str()).collect();
        assert!(ready_ids.contains(&i1.id.as_str()), "i1 should be ready");
        assert!(
            !ready_ids.contains(&i2.id.as_str()),
            "i2 should NOT be ready"
        );

        // Now move i1 to verified
        update_status(&db, &i1.id, "in_progress")
            .await
            .expect("db operation failed");
        update_status(&db, &i1.id, "in_review")
            .await
            .expect("db operation failed");
        update_status(&db, &i1.id, "verified")
            .await
            .expect("db operation failed");

        // Now i2 should be ready
        let ready = ready_intents(&db).await.expect("db operation failed");
        let ready_ids: Vec<&str> = ready.iter().map(|i| i.id.as_str()).collect();
        assert!(
            ready_ids.contains(&i2.id.as_str()),
            "i2 should now be ready"
        );
    }

    #[tokio::test]
    async fn test_get_intent_with_deps() {
        let db = crate::db::Database::open_memory()
            .await
            .expect("db operation failed");

        let i1 = Intent::new("feature", "Base", None, None);
        let i2 = Intent::new("feature", "Dependent", None, None);
        create_intent(&db, &i1).await.expect("db operation failed");
        create_intent(&db, &i2).await.expect("db operation failed");

        let edge = IntentEdge::depends_on(&i2.id, &i1.id);
        add_edge(&db, &edge).await.expect("db operation failed");

        let with_deps = get_intent_with_deps(&db, &i2.id)
            .await
            .expect("db operation failed");
        assert_eq!(with_deps.depends_on.len(), 1);
        assert_eq!(with_deps.depends_on[0].id, i1.id);
        assert!(!with_deps.all_deps_satisfied);

        let with_deps_i1 = get_intent_with_deps(&db, &i1.id)
            .await
            .expect("db operation failed");
        assert_eq!(with_deps_i1.blocks.len(), 1);
        assert_eq!(with_deps_i1.blocks[0].id, i2.id);
        assert!(with_deps_i1.all_deps_satisfied);
    }

    #[tokio::test]
    async fn test_validate_graph_no_cycles() {
        let db = crate::db::Database::open_memory()
            .await
            .expect("db operation failed");

        let i1 = Intent::new("feature", "A", None, None);
        let i2 = Intent::new("feature", "B", None, None);
        create_intent(&db, &i1).await.expect("db operation failed");
        create_intent(&db, &i2).await.expect("db operation failed");

        let edge = IntentEdge::depends_on(&i2.id, &i1.id);
        add_edge(&db, &edge).await.expect("db operation failed");

        let validation = validate_graph(&db).await.expect("db operation failed");
        assert!(!validation.has_cycles);
        assert!(validation.cycle_nodes.is_empty());
    }

    #[tokio::test]
    async fn test_self_loop_edge_rejected() {
        let db = crate::db::Database::open_memory()
            .await
            .expect("db operation failed");
        let intent = Intent::new("feature", "Self-ref", None, None);
        create_intent(&db, &intent)
            .await
            .expect("db operation failed");

        let edge = IntentEdge::depends_on(&intent.id, &intent.id);
        let result = add_edge(&db, &edge).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_intent_with_priority() {
        let db = crate::db::Database::open_memory()
            .await
            .expect("db operation failed");
        let intent = Intent::new("bug", "Critical bug", None, Some("critical".to_string()));
        create_intent(&db, &intent)
            .await
            .expect("db operation failed");

        let fetched = get_intent(&db, &intent.id)
            .await
            .expect("db operation failed");
        assert_eq!(fetched.priority, "critical");
    }

    #[tokio::test]
    async fn test_intent_with_description() {
        let db = crate::db::Database::open_memory()
            .await
            .expect("db operation failed");
        let intent = Intent::new(
            "feature",
            "Title",
            Some("Detailed description".to_string()),
            None,
        );
        create_intent(&db, &intent)
            .await
            .expect("db operation failed");

        let fetched = get_intent(&db, &intent.id)
            .await
            .expect("db operation failed");
        assert_eq!(
            fetched.description,
            Some("Detailed description".to_string())
        );
    }

    // -----------------------------------------------------------------------
    // Hash-based ID tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_hash_id_format() {
        let intent = Intent::new("feature", "Test", None, None);
        assert!(
            intent.id.starts_with("hief-"),
            "ID should start with 'hief-', got: {}",
            intent.id
        );
        assert_eq!(
            intent.id.len(),
            13,
            "ID should be 13 chars (hief- + 8 hex), got: {} ({})",
            intent.id.len(),
            intent.id
        );
    }

    #[test]
    fn test_hash_id_uniqueness() {
        let ids: Vec<String> = (0..100)
            .map(|_| Intent::new("feature", "Test", None, None).id)
            .collect();

        // All IDs should be unique
        let mut unique = ids.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(
            unique.len(),
            ids.len(),
            "All generated IDs should be unique"
        );
    }

    #[test]
    fn test_hash_id_hex_chars() {
        let intent = Intent::new("feature", "Test", None, None);
        let hex_part = &intent.id[5..]; // After "hief-"
        assert!(
            hex_part.chars().all(|c| c.is_ascii_hexdigit()),
            "Hash part should be hex, got: {}",
            hex_part
        );
    }

    #[tokio::test]
    async fn test_resolve_id_full_match() {
        let db = crate::db::Database::open_memory()
            .await
            .expect("db operation failed");
        let intent = Intent::new("feature", "Test", None, None);
        create_intent(&db, &intent)
            .await
            .expect("db operation failed");

        let resolved = resolve_id(&db, &intent.id)
            .await
            .expect("db operation failed");
        assert_eq!(resolved, intent.id);
    }

    #[tokio::test]
    async fn test_resolve_id_prefix_match() {
        let db = crate::db::Database::open_memory()
            .await
            .expect("db operation failed");
        let intent = Intent::new("feature", "Test", None, None);
        create_intent(&db, &intent)
            .await
            .expect("db operation failed");

        // Use just the first 6 chars of the hash (after "hief-")
        let short = &intent.id[..9]; // "hief-XXXX" (4 hex chars)
        let resolved = resolve_id(&db, short).await.expect("db operation failed");
        assert_eq!(resolved, intent.id);
    }

    #[tokio::test]
    async fn test_resolve_id_without_prefix() {
        let db = crate::db::Database::open_memory()
            .await
            .expect("db operation failed");
        let intent = Intent::new("feature", "Test", None, None);
        create_intent(&db, &intent)
            .await
            .expect("db operation failed");

        // Use just the hex part without "hief-"
        let hex_part = &intent.id[5..]; // 8 hex chars
        let resolved = resolve_id(&db, hex_part)
            .await
            .expect("db operation failed");
        assert_eq!(resolved, intent.id);
    }

    #[tokio::test]
    async fn test_resolve_id_not_found() {
        let db = crate::db::Database::open_memory()
            .await
            .expect("db operation failed");
        let result = resolve_id(&db, "nonexistent").await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("intent not found"), "got: {err}");
    }
}
