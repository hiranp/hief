//! Dependency edges between intents.

use serde::{Deserialize, Serialize};

use crate::db::Database;
use crate::errors::{HiefError, Result};

/// A directed edge between two intent nodes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntentEdge {
    pub from_id: String,
    pub to_id: String,
    pub kind: String,
}

impl IntentEdge {
    pub fn new(
        from_id: impl Into<String>,
        to_id: impl Into<String>,
        kind: impl Into<String>,
    ) -> Self {
        Self {
            from_id: from_id.into(),
            to_id: to_id.into(),
            kind: kind.into(),
        }
    }

    /// Create a "depends_on" edge.
    pub fn depends_on(from_id: impl Into<String>, to_id: impl Into<String>) -> Self {
        Self::new(from_id, to_id, "depends_on")
    }
}

/// Insert a new edge into the database.
pub async fn insert(db: &Database, edge: &IntentEdge) -> Result<()> {
    // Self-loop check (also enforced by SQL CHECK)
    if edge.from_id == edge.to_id {
        return Err(HiefError::DuplicateEdge {
            from: edge.from_id.clone(),
            to: edge.to_id.clone(),
            kind: edge.kind.clone(),
        });
    }

    db.conn()
        .execute(
            "INSERT INTO intent_edges (from_id, to_id, kind) VALUES (?1, ?2, ?3)",
            [
                edge.from_id.as_str(),
                edge.to_id.as_str(),
                edge.kind.as_str(),
            ],
        )
        .await
        .map_err(HiefError::Database)?;

    Ok(())
}

/// Remove an edge from the database.
#[allow(dead_code)]
pub async fn remove(db: &Database, edge: &IntentEdge) -> Result<()> {
    db.conn()
        .execute(
            "DELETE FROM intent_edges WHERE from_id = ?1 AND to_id = ?2 AND kind = ?3",
            [
                edge.from_id.as_str(),
                edge.to_id.as_str(),
                edge.kind.as_str(),
            ],
        )
        .await
        .map_err(HiefError::Database)?;

    Ok(())
}

/// List all edges for a given intent (outgoing).
#[allow(dead_code)]
pub async fn list_outgoing(db: &Database, intent_id: &str) -> Result<Vec<IntentEdge>> {
    let mut rows = db
        .conn()
        .query(
            "SELECT from_id, to_id, kind FROM intent_edges WHERE from_id = ?1",
            [intent_id],
        )
        .await
        .map_err(HiefError::Database)?;

    let mut edges = Vec::new();
    while let Some(row) = rows.next().await.map_err(HiefError::Database)? {
        edges.push(IntentEdge {
            from_id: row.get(0).map_err(HiefError::Database)?,
            to_id: row.get(1).map_err(HiefError::Database)?,
            kind: row.get(2).map_err(HiefError::Database)?,
        });
    }

    Ok(edges)
}

/// List all edges pointing to a given intent (incoming).
#[allow(dead_code)]
pub async fn list_incoming(db: &Database, intent_id: &str) -> Result<Vec<IntentEdge>> {
    let mut rows = db
        .conn()
        .query(
            "SELECT from_id, to_id, kind FROM intent_edges WHERE to_id = ?1",
            [intent_id],
        )
        .await
        .map_err(HiefError::Database)?;

    let mut edges = Vec::new();
    while let Some(row) = rows.next().await.map_err(HiefError::Database)? {
        edges.push(IntentEdge {
            from_id: row.get(0).map_err(HiefError::Database)?,
            to_id: row.get(1).map_err(HiefError::Database)?,
            kind: row.get(2).map_err(HiefError::Database)?,
        });
    }

    Ok(edges)
}
