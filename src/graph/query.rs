//! Graph queries using recursive CTEs: ready nodes, cycle detection, transitive deps.

use crate::db::Database;
use crate::errors::{HiefError, Result};
use crate::graph::intent::Intent;

/// Find intents that are `approved` and whose ALL `depends_on` targets are satisfied.
pub async fn ready_nodes(db: &Database) -> Result<Vec<Intent>> {
    let mut rows = db
        .conn()
        .query(
            "SELECT i.id, i.kind, i.title, i.description, i.status, i.priority,
                    i.criteria, i.labels, i.assigned_to, i.created_at, i.updated_at
             FROM intents i
             WHERE i.status = 'approved'
               AND NOT EXISTS (
                   SELECT 1
                   FROM intent_edges e
                   JOIN intents dep ON dep.id = e.to_id
                   WHERE e.from_id = i.id
                     AND e.kind = 'depends_on'
                     AND dep.status NOT IN ('verified', 'merged')
               )",
            (),
        )
        .await
        .map_err(HiefError::Database)?;

    let mut intents = Vec::new();
    while let Some(row) = rows.next().await.map_err(HiefError::Database)? {
        intents.push(row_to_intent(&row)?);
    }

    Ok(intents)
}

/// Check if all dependencies of an intent are satisfied.
pub async fn all_deps_satisfied(db: &Database, intent_id: &str) -> Result<bool> {
    let mut rows = db
        .conn()
        .query(
            "SELECT COUNT(*)
             FROM intent_edges e
             JOIN intents dep ON dep.id = e.to_id
             WHERE e.from_id = ?1
               AND e.kind = 'depends_on'
               AND dep.status NOT IN ('verified', 'merged')",
            [intent_id],
        )
        .await
        .map_err(HiefError::Database)?;

    if let Some(row) = rows.next().await.map_err(HiefError::Database)? {
        let count: i64 = row.get(0).map_err(HiefError::Database)?;
        Ok(count == 0)
    } else {
        Ok(true)
    }
}

/// Get all direct dependencies of an intent (nodes it depends_on).
pub async fn get_dependencies(db: &Database, intent_id: &str) -> Result<Vec<Intent>> {
    let mut rows = db
        .conn()
        .query(
            "SELECT i.id, i.kind, i.title, i.description, i.status, i.priority,
                    i.criteria, i.labels, i.assigned_to, i.created_at, i.updated_at
             FROM intents i
             JOIN intent_edges e ON e.to_id = i.id
             WHERE e.from_id = ?1 AND e.kind = 'depends_on'",
            [intent_id],
        )
        .await
        .map_err(HiefError::Database)?;

    let mut intents = Vec::new();
    while let Some(row) = rows.next().await.map_err(HiefError::Database)? {
        intents.push(row_to_intent(&row)?);
    }

    Ok(intents)
}

/// Get all intents that depend on a given intent (nodes it blocks).
pub async fn get_dependents(db: &Database, intent_id: &str) -> Result<Vec<Intent>> {
    let mut rows = db
        .conn()
        .query(
            "SELECT i.id, i.kind, i.title, i.description, i.status, i.priority,
                    i.criteria, i.labels, i.assigned_to, i.created_at, i.updated_at
             FROM intents i
             JOIN intent_edges e ON e.from_id = i.id
             WHERE e.to_id = ?1 AND e.kind = 'depends_on'",
            [intent_id],
        )
        .await
        .map_err(HiefError::Database)?;

    let mut intents = Vec::new();
    while let Some(row) = rows.next().await.map_err(HiefError::Database)? {
        intents.push(row_to_intent(&row)?);
    }

    Ok(intents)
}

/// Get all transitive dependencies (recursive CTE).
pub async fn transitive_deps(db: &Database, intent_id: &str) -> Result<Vec<Intent>> {
    let mut rows = db
        .conn()
        .query(
            "WITH RECURSIVE deps(id) AS (
                SELECT to_id FROM intent_edges WHERE from_id = ?1 AND kind = 'depends_on'
                UNION
                SELECT e.to_id FROM intent_edges e JOIN deps d ON e.from_id = d.id
                WHERE e.kind = 'depends_on'
             )
             SELECT i.id, i.kind, i.title, i.description, i.status, i.priority,
                    i.criteria, i.labels, i.assigned_to, i.created_at, i.updated_at
             FROM intents i JOIN deps d ON i.id = d.id",
            [intent_id],
        )
        .await
        .map_err(HiefError::Database)?;

    let mut intents = Vec::new();
    while let Some(row) = rows.next().await.map_err(HiefError::Database)? {
        intents.push(row_to_intent(&row)?);
    }

    Ok(intents)
}

/// Detect cycles in the dependency graph using a recursive CTE.
/// Returns IDs of nodes involved in cycles.
pub async fn detect_cycles(db: &Database) -> Result<Vec<String>> {
    let mut rows = db
        .conn()
        .query(
            "WITH RECURSIVE path(start_node, node, depth, visited) AS (
                SELECT from_id, to_id, 1, from_id || ',' || to_id
                FROM intent_edges
                WHERE kind = 'depends_on'

                UNION ALL

                SELECT p.start_node, e.to_id, p.depth + 1,
                       p.visited || ',' || e.to_id
                FROM intent_edges e
                JOIN path p ON e.from_id = p.node
                WHERE p.depth < 100
                  AND e.kind = 'depends_on'
                  AND p.start_node = e.to_id
             )
             SELECT DISTINCT start_node FROM path WHERE start_node = node",
            (),
        )
        .await
        .map_err(HiefError::Database)?;

    let mut cycle_nodes = Vec::new();
    while let Some(row) = rows.next().await.map_err(HiefError::Database)? {
        let id: String = row.get(0).map_err(HiefError::Database)?;
        cycle_nodes.push(id);
    }

    Ok(cycle_nodes)
}

/// Automatically block intents that depend on rejected nodes.
/// Returns the number of intents that were blocked.
pub async fn auto_block_rejected(db: &Database) -> Result<usize> {
    let affected = db
        .conn()
        .execute(
            "UPDATE intents
             SET status = 'blocked', updated_at = unixepoch()
             WHERE id IN (
                 SELECT e.from_id
                 FROM intent_edges e
                 JOIN intents dep ON dep.id = e.to_id
                 WHERE e.kind = 'depends_on'
                   AND dep.status = 'rejected'
             )
             AND status NOT IN ('merged', 'rejected', 'blocked')",
            (),
        )
        .await
        .map_err(HiefError::Database)?;

    Ok(affected as usize)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn row_to_intent(row: &libsql::Row) -> Result<Intent> {
    let criteria_str: String = row.get::<String>(6).unwrap_or_default();
    let criteria: Vec<String> = serde_json::from_str(&criteria_str).unwrap_or_default();

    let labels_str: String = row.get::<String>(7).unwrap_or_default();
    let labels: Vec<String> = serde_json::from_str(&labels_str).unwrap_or_default();

    let assigned_to: Option<String> = row.get::<String>(8).ok().filter(|s| !s.is_empty());

    Ok(Intent {
        id: row.get(0).map_err(HiefError::Database)?,
        kind: row.get(1).map_err(HiefError::Database)?,
        title: row.get(2).map_err(HiefError::Database)?,
        description: row.get::<String>(3).ok().filter(|s| !s.is_empty()),
        status: row.get(4).map_err(HiefError::Database)?,
        priority: row.get(5).map_err(HiefError::Database)?,
        criteria,
        labels,
        assigned_to,
        created_at: row.get(9).map_err(HiefError::Database)?,
        updated_at: row.get(10).map_err(HiefError::Database)?,
    })
}
