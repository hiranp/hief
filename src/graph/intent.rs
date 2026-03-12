//! Intent node CRUD operations.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::db::Database;
use crate::errors::{HiefError, Result};

/// An intent node — a unit of work in the dependency graph.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
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
    /// Create a new intent with a collision-free hash-based ID.
    ///
    /// IDs are generated as `hief-XXXX` where XXXX is derived from a blake3
    /// hash of a UUID v4 + timestamp, ensuring collision-free IDs across
    /// multiple concurrent agents (inspired by Beads' `bd-XXXX` scheme).
    pub fn new(
        kind: impl Into<String>,
        title: impl Into<String>,
        description: Option<String>,
        priority: Option<String>,
    ) -> Self {
        Self {
            id: generate_hash_id(),
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
    let criteria_json =
        serde_json::to_string(&intent.criteria).map_err(|e| HiefError::Other(e.to_string()))?;
    let labels_json =
        serde_json::to_string(&intent.labels).map_err(|e| HiefError::Other(e.to_string()))?;

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
pub async fn list(db: &Database, status: Option<&str>, kind: Option<&str>) -> Result<Vec<Intent>> {
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
        0 => db
            .conn()
            .query(&sql, ())
            .await
            .map_err(HiefError::Database)?,
        1 => db
            .conn()
            .query(&sql, [params[0].as_str()])
            .await
            .map_err(HiefError::Database)?,
        2 => db
            .conn()
            .query(&sql, [params[0].as_str(), params[1].as_str()])
            .await
            .map_err(HiefError::Database)?,
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

/// Generate a collision-free hash-based intent ID.
///
/// Format: `hief-XXXX` where XXXX is a hex prefix derived from blake3.
/// Uses UUID v4 + nanosecond timestamp as entropy source to prevent
/// collisions across concurrent agents. Hash prefix length starts at 4
/// and can scale (like git short hashes) if the DB grows large.
fn generate_hash_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};

    let uuid = uuid::Uuid::new_v4();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();

    let mut hasher = blake3::Hasher::new();
    hasher.update(uuid.as_bytes());
    hasher.update(&nanos.to_le_bytes());
    let hash = hasher.finalize();
    let hex = hash.to_hex();

    // Use 8 hex chars (4 bytes) — collision probability ~1 in 4 billion
    format!("hief-{}", &hex[..8])
}

/// Resolve a possibly-short intent ID to a full ID via prefix matching.
///
/// Accepts full IDs (`hief-a1b2c3d4`) or short prefixes (`hief-a1b2`, `a1b2`).
/// Returns an error if the prefix is ambiguous (matches multiple intents).
pub async fn resolve_id(db: &Database, id_or_prefix: &str) -> Result<String> {
    // If it's already a full match, return it
    if let Ok(intent) = get(db, id_or_prefix).await {
        return Ok(intent.id);
    }

    // Try prefix match (with or without "hief-" prefix)
    let prefix = if id_or_prefix.starts_with("hief-") {
        id_or_prefix.to_string()
    } else {
        format!("hief-{}", id_or_prefix)
    };

    let mut rows = db
        .conn()
        .query(
            "SELECT id FROM intents WHERE id LIKE ?1",
            [format!("{}%", prefix)],
        )
        .await
        .map_err(HiefError::Database)?;

    let mut matches = Vec::new();
    while let Some(row) = rows.next().await.map_err(HiefError::Database)? {
        let id: String = row.get(0).map_err(HiefError::Database)?;
        matches.push(id);
    }

    match matches.len() {
        0 => Err(HiefError::IntentNotFound(id_or_prefix.to_string())),
        1 => Ok(matches.into_iter().next().expect("one match present")),
        _ => Err(HiefError::AmbiguousId {
            prefix: id_or_prefix.to_string(),
            matches,
        }),
    }
}

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
