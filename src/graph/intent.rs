//! Intent node CRUD operations.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::db::Database;
use crate::errors::{HiefError, Result};

/// An intent node — a unit of work in the dependency graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Intent {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub description: Option<String>,
    pub status: String,
    pub priority: String,
    pub criteria: Vec<String>,
    pub labels: Vec<String>,
    pub assigned_to: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

impl Intent {
    /// Create a new intent with generated UUID.
    pub fn new(
        kind: impl Into<String>,
        title: impl Into<String>,
        description: Option<String>,
        priority: Option<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            kind: kind.into(),
            title: title.into(),
            description,
            status: "draft".to_string(),
            priority: priority.unwrap_or_else(|| "medium".to_string()),
            criteria: Vec::new(),
            labels: Vec::new(),
            assigned_to: None,
            created_at: 0, // set by DB
            updated_at: 0, // set by DB
        }
    }
}

/// Insert a new intent into the database.
pub async fn insert(db: &Database, intent: &Intent) -> Result<()> {
    let criteria_json = serde_json::to_string(&intent.criteria)
        .map_err(|e| HiefError::Other(e.to_string()))?;
    let labels_json = serde_json::to_string(&intent.labels)
        .map_err(|e| HiefError::Other(e.to_string()))?;

    db.conn()
        .execute(
            "INSERT INTO intents (id, kind, title, description, status, priority, criteria, labels, assigned_to)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            libsql::params![
                intent.id.as_str(),
                intent.kind.as_str(),
                intent.title.as_str(),
                intent.description.as_deref().unwrap_or(""),
                intent.status.as_str(),
                intent.priority.as_str(),
                criteria_json.as_str(),
                labels_json.as_str(),
                intent.assigned_to.as_deref().unwrap_or(""),
            ],
        )
        .await
        .map_err(HiefError::Database)?;

    Ok(())
}

/// Get an intent by ID.
pub async fn get(db: &Database, id: &str) -> Result<Intent> {
    let mut rows = db
        .conn()
        .query(
            "SELECT id, kind, title, description, status, priority, criteria, labels, assigned_to, created_at, updated_at
             FROM intents WHERE id = ?1",
            [id],
        )
        .await
        .map_err(HiefError::Database)?;

    let row = rows
        .next()
        .await
        .map_err(HiefError::Database)?
        .ok_or_else(|| HiefError::IntentNotFound(id.to_string()))?;

    row_to_intent(&row)
}

/// List intents with optional status and kind filters.
pub async fn list(
    db: &Database,
    status: Option<&str>,
    kind: Option<&str>,
) -> Result<Vec<Intent>> {
    let mut sql = String::from(
        "SELECT id, kind, title, description, status, priority, criteria, labels, assigned_to, created_at, updated_at
         FROM intents WHERE 1=1",
    );

    let mut params: Vec<String> = Vec::new();
    let mut param_idx = 1;

    if let Some(s) = status {
        sql.push_str(&format!(" AND status = ?{}", param_idx));
        params.push(s.to_string());
        param_idx += 1;
    }

    if let Some(k) = kind {
        sql.push_str(&format!(" AND kind = ?{}", param_idx));
        params.push(k.to_string());
        param_idx += 1;
    }

    let _ = param_idx;
    sql.push_str(" ORDER BY created_at DESC");

    let mut rows = match params.len() {
        0 => db.conn().query(&sql, ()).await.map_err(HiefError::Database)?,
        1 => {
            db.conn()
                .query(&sql, [params[0].as_str()])
                .await
                .map_err(HiefError::Database)?
        }
        2 => {
            db.conn()
                .query(&sql, [params[0].as_str(), params[1].as_str()])
                .await
                .map_err(HiefError::Database)?
        }
        _ => unreachable!(),
    };

    let mut intents = Vec::new();
    while let Some(row) = rows.next().await.map_err(HiefError::Database)? {
        intents.push(row_to_intent(&row)?);
    }

    Ok(intents)
}

/// Update the status of an intent.
pub async fn update_status(db: &Database, id: &str, new_status: &str) -> Result<()> {
    let affected = db
        .conn()
        .execute(
            "UPDATE intents SET status = ?1, updated_at = unixepoch() WHERE id = ?2",
            [new_status, id],
        )
        .await
        .map_err(HiefError::Database)?;

    if affected == 0 {
        return Err(HiefError::IntentNotFound(id.to_string()));
    }

    Ok(())
}

/// Assign an intent to an agent or human.
pub async fn assign(db: &Database, id: &str, assigned_to: &str) -> Result<()> {
    let affected = db
        .conn()
        .execute(
            "UPDATE intents SET assigned_to = ?1, updated_at = unixepoch() WHERE id = ?2",
            [assigned_to, id],
        )
        .await
        .map_err(HiefError::Database)?;

    if affected == 0 {
        return Err(HiefError::IntentNotFound(id.to_string()));
    }

    Ok(())
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
