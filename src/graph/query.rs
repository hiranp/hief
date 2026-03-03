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
#[allow(dead_code)]
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
            "SELECT from_id, to_id FROM intent_edges WHERE kind = 'depends_on'",
            (),
        )
        .await
        .map_err(HiefError::Database)?;

    let mut adj: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();
    let mut nodes: std::collections::HashSet<String> = std::collections::HashSet::new();

    while let Some(row) = rows.next().await.map_err(HiefError::Database)? {
        let from: String = row.get(0).unwrap();
        let to: String = row.get(1).unwrap();
        adj.entry(from.clone()).or_default().push(to.clone());
        nodes.insert(from);
        nodes.insert(to);
    }

    let mut index = 0;
    let mut stack: Vec<String> = Vec::new();
    let mut on_stack: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut indices: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let mut lowlinks: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let mut cycle_nodes = std::collections::HashSet::new();

    fn strongconnect(
        v: &String,
        index: &mut usize,
        stack: &mut Vec<String>,
        on_stack: &mut std::collections::HashSet<String>,
        indices: &mut std::collections::HashMap<String, usize>,
        lowlinks: &mut std::collections::HashMap<String, usize>,
        adj: &std::collections::HashMap<String, Vec<String>>,
        cycle_nodes: &mut std::collections::HashSet<String>,
    ) {
        indices.insert(v.clone(), *index);
        lowlinks.insert(v.clone(), *index);
        *index += 1;
        stack.push(v.clone());
        on_stack.insert(v.clone());

        if let Some(neighbors) = adj.get(v) {
            for w in neighbors {
                if !indices.contains_key(w) {
                    strongconnect(
                        w,
                        index,
                        stack,
                        on_stack,
                        indices,
                        lowlinks,
                        adj,
                        cycle_nodes,
                    );
                    let low_v = *lowlinks.get(v).unwrap();
                    let low_w = *lowlinks.get(w).unwrap();
                    lowlinks.insert(v.clone(), std::cmp::min(low_v, low_w));
                } else if on_stack.contains(w) {
                    let low_v = *lowlinks.get(v).unwrap();
                    let idx_w = *indices.get(w).unwrap();
                    lowlinks.insert(v.clone(), std::cmp::min(low_v, idx_w));
                }
            }
        }

        if lowlinks.get(v) == indices.get(v) {
            let mut scc = Vec::new();
            loop {
                let w = stack.pop().unwrap();
                on_stack.remove(&w);
                scc.push(w.clone());
                if w == *v {
                    break;
                }
            }
            if scc.len() > 1 {
                for node in scc {
                    cycle_nodes.insert(node);
                }
            } else if let Some(neighbors) = adj.get(v) {
                if neighbors.contains(v) {
                    cycle_nodes.insert(v.clone());
                }
            }
        }
    }

    for v in &nodes {
        if !indices.contains_key(v) {
            strongconnect(
                v,
                &mut index,
                &mut stack,
                &mut on_stack,
                &mut indices,
                &mut lowlinks,
                &adj,
                &mut cycle_nodes,
            );
        }
    }

    let mut result: Vec<String> = cycle_nodes.into_iter().collect();
    result.sort();
    Ok(result)
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
