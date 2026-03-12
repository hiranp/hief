//! Cognitive memory layer: access tracking, co-access graphs, and activation-weighted search.
//!
//! This module implements adaptive memory for HIEF, inspired by MuninnDB's
//! cognitive memory model but using simple SQL and math (no LLM, no neural networks).
//!
//! # Key concepts
//!
//! - **Access tracking**: Records every time an agent retrieves code via search tools.
//! - **Co-access graph (Hebbian)**: Tracks which files are frequently accessed together,
//!   strengthening connections with use and decaying them over time.
//! - **Activation-weighted search**: Boosts search results by access history so that
//!   recently and frequently accessed code ranks higher.
//! - **Related files**: Uses the co-access graph to suggest files related to a given file.

use schemars::JsonSchema;
use serde::Serialize;
use tracing::debug;
use uuid::Uuid;

use crate::db::Database;
use crate::errors::{HiefError, Result};

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// A record of a code chunk being accessed by an agent.
#[derive(Debug, Clone, Serialize)]
#[allow(dead_code)]
pub struct AccessRecord {
    pub id: String,
    pub chunk_id: Option<String>,
    pub file_path: String,
    pub query: Option<String>,
    pub tool: String,
    pub session_id: Option<String>,
    pub accessed_at: i64,
}

/// A co-access edge between two files.
#[derive(Debug, Clone, Serialize)]
#[allow(dead_code)]
pub struct CoAccessEdge {
    pub file_a: String,
    pub file_b: String,
    pub strength: f64,
    pub last_co_access: i64,
}

/// A related file suggestion from the co-access graph.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct RelatedFile {
    pub file_path: String,
    pub strength: f64,
    pub last_co_access: i64,
}

/// Access statistics for a file (used for search boosting).
#[derive(Debug, Clone, Serialize)]
#[allow(dead_code)]
pub struct FileAccessStats {
    pub file_path: String,
    pub access_count: i64,
    pub last_accessed: i64,
}

/// Session context: files accessed this session plus related suggestions.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct SessionContext {
    /// Files accessed in the current session with access counts.
    pub accessed_files: Vec<SessionFileAccess>,
    /// Related files not yet accessed (from co-access graph).
    pub suggested_files: Vec<RelatedFile>,
    /// Total access count this session.
    pub total_accesses: i64,
}

/// A file access entry within a session.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct SessionFileAccess {
    pub file_path: String,
    pub access_count: i64,
    pub last_accessed: i64,
}

// ---------------------------------------------------------------------------
// Access recording (Step 2)
// ---------------------------------------------------------------------------

/// Record an access event for a search result.
///
/// Called by MCP tool handlers after returning search results to the agent.
pub async fn record_access(
    db: &Database,
    file_path: &str,
    chunk_id: Option<&str>,
    query: Option<&str>,
    tool: &str,
    session_id: Option<&str>,
) -> Result<String> {
    let id = Uuid::new_v4().to_string();
    let conn = db.conn();

    conn.execute(
        "INSERT INTO chunk_access (id, chunk_id, file_path, query, tool, session_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        libsql::params![
            id.as_str(),
            chunk_id.unwrap_or(""),
            file_path,
            query.unwrap_or(""),
            tool,
            session_id.unwrap_or(""),
        ],
    )
    .await
    .map_err(HiefError::Database)?;

    debug!(
        "Recorded access: file={}, tool={}, session={:?}",
        file_path, tool, session_id
    );

    Ok(id)
}

/// Record accesses for multiple files from a single search operation.
///
/// Also triggers co-access graph updates for all file pairs in the result set.
pub async fn record_search_accesses(
    db: &Database,
    file_paths: &[String],
    query: Option<&str>,
    tool: &str,
    session_id: Option<&str>,
) -> Result<()> {
    // Record individual accesses
    for path in file_paths {
        record_access(db, path, None, query, tool, session_id).await?;
    }

    // Update co-access graph for all pairs of files in the result set
    update_co_access_from_results(db, file_paths).await?;

    // Also update co-access based on session proximity
    if let Some(sid) = session_id {
        update_co_access_from_session(db, sid).await?;
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Co-access graph (Step 3)
// ---------------------------------------------------------------------------

/// Update co-access strength for all pairs of files in a search result set.
///
/// When files appear together in search results, they are likely related.
/// Uses Hebbian learning: `strength = strength * 0.95 + 1.0`
async fn update_co_access_from_results(db: &Database, file_paths: &[String]) -> Result<()> {
    // Deduplicate file paths
    let mut unique_paths: Vec<&str> = file_paths.iter().map(|s| s.as_str()).collect();
    unique_paths.sort();
    unique_paths.dedup();

    // Update co-access for all pairs (limit to first 10 to avoid quadratic explosion)
    let paths_to_process = if unique_paths.len() > 10 {
        &unique_paths[..10]
    } else {
        &unique_paths
    };

    for i in 0..paths_to_process.len() {
        for j in (i + 1)..paths_to_process.len() {
            let (file_a, file_b) = ordered_pair(paths_to_process[i], paths_to_process[j]);
            upsert_co_access(db, file_a, file_b).await?;
        }
    }

    Ok(())
}

/// Update co-access based on files accessed in the same session (within last 5 minutes).
///
/// Looks at recent accesses in the same session and strengthens connections
/// between files that were accessed close together in time.
async fn update_co_access_from_session(db: &Database, session_id: &str) -> Result<()> {
    let conn = db.conn();

    // Get distinct files accessed in this session within the last 5 minutes
    let mut rows = conn
        .query(
            "SELECT DISTINCT file_path FROM chunk_access
             WHERE session_id = ?1 AND accessed_at >= (unixepoch() - 300)
             ORDER BY accessed_at DESC
             LIMIT 20",
            [session_id],
        )
        .await
        .map_err(HiefError::Database)?;

    let mut session_files: Vec<String> = Vec::new();
    while let Some(row) = rows.next().await.map_err(HiefError::Database)? {
        let path: String = row.get(0).map_err(HiefError::Database)?;
        if !path.is_empty() {
            session_files.push(path);
        }
    }

    // Update co-access for all pairs (limit to 10 most recent)
    let limit = session_files.len().min(10);
    for i in 0..limit {
        for j in (i + 1)..limit {
            let (file_a, file_b) =
                ordered_pair(session_files[i].as_str(), session_files[j].as_str());
            upsert_co_access(db, file_a, file_b).await?;
        }
    }

    Ok(())
}

/// Insert or update a co-access edge using Hebbian learning rule.
///
/// `strength = strength * 0.95 + 1.0` (exponential decay + increment)
async fn upsert_co_access(db: &Database, file_a: &str, file_b: &str) -> Result<()> {
    let conn = db.conn();

    // Try to update existing edge with Hebbian rule
    let updated = conn
        .execute(
            "UPDATE co_access
             SET strength = strength * 0.95 + 1.0,
                 last_co_access = unixepoch()
             WHERE file_a = ?1 AND file_b = ?2",
            [file_a, file_b],
        )
        .await
        .map_err(HiefError::Database)?;

    // If no row existed, insert a new one
    if updated == 0 {
        conn.execute(
            "INSERT OR IGNORE INTO co_access (file_a, file_b, strength, last_co_access)
             VALUES (?1, ?2, 1.0, unixepoch())",
            [file_a, file_b],
        )
        .await
        .map_err(HiefError::Database)?;
    }

    Ok(())
}

/// Ensure consistent ordering of file pairs for the co-access graph.
///
/// Always stores (file_a, file_b) where file_a < file_b alphabetically.
fn ordered_pair<'a>(a: &'a str, b: &'a str) -> (&'a str, &'a str) {
    if a <= b { (a, b) } else { (b, a) }
}

// ---------------------------------------------------------------------------
// Related files (Step 5)
// ---------------------------------------------------------------------------

/// Get files related to the given file, ranked by co-access strength.
///
/// Queries the co-access graph for all edges involving the given file,
/// returning the connected files sorted by strength (strongest first).
pub async fn related_files(
    db: &Database,
    file_path: &str,
    top_k: usize,
) -> Result<Vec<RelatedFile>> {
    let conn = db.conn();

    let mut rows = conn
        .query(
            "SELECT
                CASE WHEN file_a = ?1 THEN file_b ELSE file_a END as related,
                strength,
                last_co_access
             FROM co_access
             WHERE file_a = ?1 OR file_b = ?1
             ORDER BY strength DESC
             LIMIT ?2",
            libsql::params![file_path, top_k as i64],
        )
        .await
        .map_err(HiefError::Database)?;

    let mut results = Vec::new();
    while let Some(row) = rows.next().await.map_err(HiefError::Database)? {
        let related_path: String = row.get(0).map_err(HiefError::Database)?;
        let strength: f64 = row.get(1).map_err(HiefError::Database)?;
        let last_co_access: i64 = row.get(2).map_err(HiefError::Database)?;

        results.push(RelatedFile {
            file_path: related_path,
            strength,
            last_co_access,
        });
    }

    debug!("Found {} related files for '{}'", results.len(), file_path);
    Ok(results)
}

// ---------------------------------------------------------------------------
// Activation-weighted search boost (Step 4)
// ---------------------------------------------------------------------------

/// Compute the access boost for a given file path.
///
/// Formula: `access_boost = log(1 + access_count) * recency_weight`
/// where `recency_weight = exp(-days_since_last_access / 30.0)`
///
/// This means:
/// - Recently accessed code gets a higher boost
/// - Frequently accessed code gets a boost with diminishing returns (log)
/// - The boost decays over ~30 days
#[allow(dead_code)]
pub async fn compute_access_boost(db: &Database, file_path: &str) -> Result<f64> {
    let conn = db.conn();

    let mut rows = conn
        .query(
            "SELECT COUNT(*) as access_count,
                    MAX(accessed_at) as last_accessed
             FROM chunk_access
             WHERE file_path = ?1",
            [file_path],
        )
        .await
        .map_err(HiefError::Database)?;

    if let Some(row) = rows.next().await.map_err(HiefError::Database)? {
        let access_count: i64 = row.get(0).map_err(HiefError::Database)?;
        let last_accessed: Option<i64> = row.get(1).ok();

        if access_count == 0 {
            return Ok(0.0);
        }

        let now = chrono::Utc::now().timestamp();
        let days_since = last_accessed
            .map(|la| (now - la) as f64 / 86400.0)
            .unwrap_or(30.0);

        let boost = (1.0 + access_count as f64).ln() * (-days_since / 30.0_f64).exp();
        Ok(boost)
    } else {
        Ok(0.0)
    }
}

/// Get access boost values for multiple files at once (batch query for search).
///
/// Returns a map of file_path → access_boost for efficient lookup during
/// search result re-ranking.
pub async fn batch_access_boost(
    db: &Database,
    file_paths: &[String],
) -> Result<std::collections::HashMap<String, f64>> {
    let mut boosts = std::collections::HashMap::new();

    if file_paths.is_empty() {
        return Ok(boosts);
    }

    let conn = db.conn();

    // Query access stats for all relevant files in one go
    // Using a single query with GROUP BY is more efficient than per-file queries
    let mut rows = conn
        .query(
            "SELECT file_path, COUNT(*) as access_count, MAX(accessed_at) as last_accessed
             FROM chunk_access
             WHERE file_path IN (SELECT DISTINCT file_path FROM chunk_access)
             GROUP BY file_path",
            (),
        )
        .await
        .map_err(HiefError::Database)?;

    let now = chrono::Utc::now().timestamp();

    while let Some(row) = rows.next().await.map_err(HiefError::Database)? {
        let path: String = row.get(0).map_err(HiefError::Database)?;
        let access_count: i64 = row.get(1).map_err(HiefError::Database)?;
        let last_accessed: i64 = row.get(2).map_err(HiefError::Database)?;

        if file_paths.contains(&path) {
            let days_since = (now - last_accessed) as f64 / 86400.0;
            let boost = (1.0 + access_count as f64).ln() * (-days_since / 30.0_f64).exp();
            boosts.insert(path, boost);
        }
    }

    Ok(boosts)
}

// ---------------------------------------------------------------------------
// Session context (Step 6)
// ---------------------------------------------------------------------------

/// Build session context: files accessed this session plus related suggestions.
///
/// This powers the `project://session-context` MCP resource.
pub async fn get_session_context(
    db: &Database,
    session_id: &str,
    suggestion_limit: usize,
) -> Result<SessionContext> {
    let conn = db.conn();

    // Get files accessed this session with counts
    let mut rows = conn
        .query(
            "SELECT file_path, COUNT(*) as cnt, MAX(accessed_at) as last_access
             FROM chunk_access
             WHERE session_id = ?1
             GROUP BY file_path
             ORDER BY last_access DESC",
            [session_id],
        )
        .await
        .map_err(HiefError::Database)?;

    let mut accessed_files = Vec::new();
    let mut total_accesses: i64 = 0;
    let mut accessed_paths: Vec<String> = Vec::new();

    while let Some(row) = rows.next().await.map_err(HiefError::Database)? {
        let file_path: String = row.get(0).map_err(HiefError::Database)?;
        let count: i64 = row.get(1).map_err(HiefError::Database)?;
        let last_accessed: i64 = row.get(2).map_err(HiefError::Database)?;

        total_accesses += count;
        accessed_paths.push(file_path.clone());

        accessed_files.push(SessionFileAccess {
            file_path,
            access_count: count,
            last_accessed,
        });
    }

    // Get related files not yet accessed in this session
    let mut suggested_files = Vec::new();
    for accessed_path in &accessed_paths {
        let related = related_files(db, accessed_path, suggestion_limit).await?;
        for rf in related {
            // Only suggest files not already accessed in this session
            if !accessed_paths.contains(&rf.file_path)
                && !suggested_files
                    .iter()
                    .any(|s: &RelatedFile| s.file_path == rf.file_path)
            {
                suggested_files.push(rf);
            }
        }
    }

    // Sort suggestions by strength and limit
    suggested_files.sort_by(|a, b| {
        b.strength
            .partial_cmp(&a.strength)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    suggested_files.truncate(suggestion_limit);

    Ok(SessionContext {
        accessed_files,
        suggested_files,
        total_accesses,
    })
}

/// Get access statistics for all files (used by the health/overview resources).
#[allow(dead_code)]
pub async fn get_access_stats(db: &Database, limit: usize) -> Result<Vec<FileAccessStats>> {
    let conn = db.conn();

    let mut rows = conn
        .query(
            "SELECT file_path, COUNT(*) as cnt, MAX(accessed_at) as last_access
             FROM chunk_access
             GROUP BY file_path
             ORDER BY cnt DESC
             LIMIT ?1",
            [limit as i64],
        )
        .await
        .map_err(HiefError::Database)?;

    let mut stats = Vec::new();
    while let Some(row) = rows.next().await.map_err(HiefError::Database)? {
        let file_path: String = row.get(0).map_err(HiefError::Database)?;
        let access_count: i64 = row.get(1).map_err(HiefError::Database)?;
        let last_accessed: i64 = row.get(2).map_err(HiefError::Database)?;

        stats.push(FileAccessStats {
            file_path,
            access_count,
            last_accessed,
        });
    }

    Ok(stats)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_record_access() {
        let db = Database::open_memory().await.unwrap();

        let id = record_access(
            &db,
            "src/main.rs",
            Some("chunk-1"),
            Some("fn main"),
            "search_code",
            Some("session-1"),
        )
        .await
        .unwrap();

        assert!(!id.is_empty());

        // Verify the record exists
        let mut rows = db
            .conn()
            .query(
                "SELECT file_path, tool FROM chunk_access WHERE id = ?1",
                [id.as_str()],
            )
            .await
            .unwrap();
        let row = rows.next().await.unwrap().unwrap();
        let file_path: String = row.get(0).unwrap();
        let tool: String = row.get(1).unwrap();
        assert_eq!(file_path, "src/main.rs");
        assert_eq!(tool, "search_code");
    }

    #[tokio::test]
    async fn test_record_search_accesses_creates_co_access() {
        let db = Database::open_memory().await.unwrap();

        let files = vec![
            "src/db.rs".to_string(),
            "src/errors.rs".to_string(),
            "src/config.rs".to_string(),
        ];

        record_search_accesses(&db, &files, Some("database"), "search_code", Some("s1"))
            .await
            .unwrap();

        // Check co-access edges were created
        let mut rows = db
            .conn()
            .query("SELECT COUNT(*) FROM co_access", ())
            .await
            .unwrap();
        let row = rows.next().await.unwrap().unwrap();
        let count: i64 = row.get(0).unwrap();
        // 3 files → 3 pairs: (config, db), (config, errors), (db, errors)
        assert_eq!(count, 3);
    }

    #[tokio::test]
    async fn test_co_access_hebbian_update() {
        let db = Database::open_memory().await.unwrap();

        // First access pair
        upsert_co_access(&db, "src/a.rs", "src/b.rs").await.unwrap();

        let mut rows = db
            .conn()
            .query(
                "SELECT strength FROM co_access WHERE file_a = 'src/a.rs' AND file_b = 'src/b.rs'",
                (),
            )
            .await
            .unwrap();
        let row = rows.next().await.unwrap().unwrap();
        let s1: f64 = row.get(0).unwrap();
        assert!(
            (s1 - 1.0).abs() < f64::EPSILON,
            "Initial strength should be 1.0"
        );

        // Second access: strength = 1.0 * 0.95 + 1.0 = 1.95
        upsert_co_access(&db, "src/a.rs", "src/b.rs").await.unwrap();

        let mut rows = db
            .conn()
            .query(
                "SELECT strength FROM co_access WHERE file_a = 'src/a.rs' AND file_b = 'src/b.rs'",
                (),
            )
            .await
            .unwrap();
        let row = rows.next().await.unwrap().unwrap();
        let s2: f64 = row.get(0).unwrap();
        assert!(
            (s2 - 1.95).abs() < 0.01,
            "After second access, strength should be ~1.95, got {}",
            s2
        );

        // Third access: strength = 1.95 * 0.95 + 1.0 = 2.8525
        upsert_co_access(&db, "src/a.rs", "src/b.rs").await.unwrap();

        let mut rows = db
            .conn()
            .query(
                "SELECT strength FROM co_access WHERE file_a = 'src/a.rs' AND file_b = 'src/b.rs'",
                (),
            )
            .await
            .unwrap();
        let row = rows.next().await.unwrap().unwrap();
        let s3: f64 = row.get(0).unwrap();
        assert!(
            (s3 - 2.8525).abs() < 0.01,
            "After third access, strength should be ~2.8525, got {}",
            s3
        );
    }

    #[tokio::test]
    async fn test_ordered_pair() {
        assert_eq!(ordered_pair("b.rs", "a.rs"), ("a.rs", "b.rs"));
        assert_eq!(ordered_pair("a.rs", "b.rs"), ("a.rs", "b.rs"));
        assert_eq!(ordered_pair("x.rs", "x.rs"), ("x.rs", "x.rs"));
    }

    #[tokio::test]
    async fn test_related_files() {
        let db = Database::open_memory().await.unwrap();

        // Build a co-access graph
        upsert_co_access(&db, "src/db.rs", "src/errors.rs")
            .await
            .unwrap();
        upsert_co_access(&db, "src/db.rs", "src/errors.rs")
            .await
            .unwrap(); // strengthen
        upsert_co_access(&db, "src/db.rs", "src/config.rs")
            .await
            .unwrap();

        let related = related_files(&db, "src/db.rs", 10).await.unwrap();

        assert_eq!(related.len(), 2);
        // errors.rs should be ranked higher (strength ~1.95 vs 1.0)
        assert_eq!(related[0].file_path, "src/errors.rs");
        assert_eq!(related[1].file_path, "src/config.rs");
        assert!(related[0].strength > related[1].strength);
    }

    #[tokio::test]
    async fn test_related_files_bidirectional() {
        let db = Database::open_memory().await.unwrap();

        // Store with ordered pair (a < b alphabetically)
        upsert_co_access(&db, "src/alpha.rs", "src/beta.rs")
            .await
            .unwrap();

        // Query from both sides should work
        let from_alpha = related_files(&db, "src/alpha.rs", 10).await.unwrap();
        assert_eq!(from_alpha.len(), 1);
        assert_eq!(from_alpha[0].file_path, "src/beta.rs");

        let from_beta = related_files(&db, "src/beta.rs", 10).await.unwrap();
        assert_eq!(from_beta.len(), 1);
        assert_eq!(from_beta[0].file_path, "src/alpha.rs");
    }

    #[tokio::test]
    async fn test_compute_access_boost_no_history() {
        let db = Database::open_memory().await.unwrap();

        let boost = compute_access_boost(&db, "nonexistent.rs").await.unwrap();
        assert!(
            (boost - 0.0).abs() < f64::EPSILON,
            "No access history should give 0 boost"
        );
    }

    #[tokio::test]
    async fn test_compute_access_boost_with_accesses() {
        let db = Database::open_memory().await.unwrap();

        // Record some accesses
        for _ in 0..5 {
            record_access(&db, "src/hot.rs", None, Some("test"), "search_code", None)
                .await
                .unwrap();
        }

        let boost = compute_access_boost(&db, "src/hot.rs").await.unwrap();
        // access_count=5, just now so recency_weight ≈ 1.0
        // boost = ln(1+5) * exp(0) = ln(6) * 1 ≈ 1.79
        assert!(
            boost > 1.0,
            "Boost for recently accessed file should be > 1.0, got {}",
            boost
        );
        assert!(boost < 3.0, "Boost should be reasonable, got {}", boost);
    }

    #[tokio::test]
    async fn test_session_context() {
        let db = Database::open_memory().await.unwrap();

        let session = "test-session-42";

        // Record accesses in this session
        record_access(
            &db,
            "src/db.rs",
            None,
            Some("database"),
            "search_code",
            Some(session),
        )
        .await
        .unwrap();
        record_access(
            &db,
            "src/db.rs",
            None,
            Some("query"),
            "search_code",
            Some(session),
        )
        .await
        .unwrap();
        record_access(
            &db,
            "src/errors.rs",
            None,
            Some("error"),
            "search_code",
            Some(session),
        )
        .await
        .unwrap();

        // Add some co-access data
        upsert_co_access(&db, "src/config.rs", "src/db.rs")
            .await
            .unwrap();

        let ctx = get_session_context(&db, session, 10).await.unwrap();

        assert_eq!(ctx.accessed_files.len(), 2);
        assert_eq!(ctx.total_accesses, 3);

        // db.rs should have count=2
        let db_access = ctx
            .accessed_files
            .iter()
            .find(|f| f.file_path == "src/db.rs")
            .unwrap();
        assert_eq!(db_access.access_count, 2);

        // config.rs should be suggested (co-accessed with db.rs but not accessed this session)
        let suggested_paths: Vec<&str> = ctx
            .suggested_files
            .iter()
            .map(|f| f.file_path.as_str())
            .collect();
        assert!(
            suggested_paths.contains(&"src/config.rs"),
            "config.rs should be suggested via co-access with db.rs, got {:?}",
            suggested_paths
        );
    }

    #[tokio::test]
    async fn test_get_access_stats() {
        let db = Database::open_memory().await.unwrap();

        // Record varying access counts
        for _ in 0..5 {
            record_access(&db, "src/hot.rs", None, None, "search_code", None)
                .await
                .unwrap();
        }
        for _ in 0..2 {
            record_access(&db, "src/warm.rs", None, None, "search_code", None)
                .await
                .unwrap();
        }
        record_access(&db, "src/cold.rs", None, None, "search_code", None)
            .await
            .unwrap();

        let stats = get_access_stats(&db, 10).await.unwrap();

        assert_eq!(stats.len(), 3);
        // Should be ordered by count descending
        assert_eq!(stats[0].file_path, "src/hot.rs");
        assert_eq!(stats[0].access_count, 5);
        assert_eq!(stats[1].file_path, "src/warm.rs");
        assert_eq!(stats[1].access_count, 2);
        assert_eq!(stats[2].file_path, "src/cold.rs");
        assert_eq!(stats[2].access_count, 1);
    }

    #[tokio::test]
    async fn test_batch_access_boost() {
        let db = Database::open_memory().await.unwrap();

        for _ in 0..3 {
            record_access(&db, "src/a.rs", None, None, "search_code", None)
                .await
                .unwrap();
        }
        record_access(&db, "src/b.rs", None, None, "search_code", None)
            .await
            .unwrap();

        let paths = vec![
            "src/a.rs".to_string(),
            "src/b.rs".to_string(),
            "src/c.rs".to_string(),
        ];
        let boosts = batch_access_boost(&db, &paths).await.unwrap();

        assert!(boosts.get("src/a.rs").unwrap() > boosts.get("src/b.rs").unwrap());
        assert!(!boosts.contains_key("src/c.rs")); // No access history
    }

    #[tokio::test]
    async fn test_cognitive_memory_tables_exist() {
        let db = Database::open_memory().await.unwrap();

        // Verify chunk_access table
        db.conn()
            .execute(
                "INSERT INTO chunk_access (id, file_path, tool) VALUES ('test', 'test.rs', 'search_code')",
                (),
            )
            .await
            .unwrap();

        // Verify co_access table
        db.conn()
            .execute(
                "INSERT INTO co_access (file_a, file_b) VALUES ('a.rs', 'b.rs')",
                (),
            )
            .await
            .unwrap();

        // Clean up
        let mut rows = db
            .conn()
            .query("SELECT COUNT(*) FROM chunk_access", ())
            .await
            .unwrap();
        let row = rows.next().await.unwrap().unwrap();
        let count: i64 = row.get(0).unwrap();
        assert_eq!(count, 1);
    }
}
