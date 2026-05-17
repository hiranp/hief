//! Database connection and migrations for libsql.

use libsql::{Builder, Connection};
use serde::{Deserialize, Serialize};
use std::path::Path;
use tracing::{debug, info};

use crate::errors::{HiefError, Result};
use crate::scope;

const MAX_SESSION_ID_LEN: usize = 128;
const MAX_TOOL_LEN: usize = 128;
const MAX_QUERY_LEN: usize = 2048;
const MAX_STRATEGY_LEN: usize = 512;
/// Wrapper around libsql connection providing migration support.
#[derive(Clone)]
pub struct Database {
    conn: Connection,
}

impl Database {
    /// Open (or create) the database at the given path.
    pub async fn open(path: &Path) -> Result<Self> {
        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let db = Builder::new_local(path)
            .build()
            .await
            .map_err(HiefError::Database)?;

        let conn = db.connect().map_err(HiefError::Database)?;

        // Enable WAL mode and NORMAL synchronous for concurrent read/write safety and performance
        // Use query (not execute) because PRAGMA journal_mode returns a result row
        let _ = conn
            .query("PRAGMA journal_mode=WAL", ())
            .await
            .map_err(HiefError::Database)?;
        let _ = conn
            .execute("PRAGMA synchronous=NORMAL", ())
            .await
            .map_err(HiefError::Database)?;
        let _ = conn
            .query("PRAGMA foreign_keys=ON", ())
            .await
            .map_err(HiefError::Database)?;

        let database = Self { conn };
        database.run_migrations().await?;
        Ok(database)
    }

    /// Open an in-memory database (for testing).
    #[cfg(test)]
    pub async fn open_memory() -> Result<Self> {
        let db = Builder::new_local(":memory:")
            .build()
            .await
            .map_err(HiefError::Database)?;

        let conn = db.connect().map_err(HiefError::Database)?;
        conn.execute("PRAGMA foreign_keys=ON", ())
            .await
            .map_err(HiefError::Database)?;

        let database = Self { conn };
        database.run_migrations().await?;
        Ok(database)
    }

    /// Get a reference to the underlying connection.
    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    /// Run all schema migrations.
    async fn run_migrations(&self) -> Result<()> {
        info!("Running database migrations");

        // Migration tracking table
        self.conn
            .execute(
                "CREATE TABLE IF NOT EXISTS _migrations (
                    id INTEGER PRIMARY KEY,
                    name TEXT NOT NULL UNIQUE,
                    applied_at INTEGER DEFAULT (unixepoch())
                )",
                (),
            )
            .await
            .map_err(HiefError::Database)?;

        let migrations: &[(&str, &str)] = &[
            ("001_chunks", MIGRATION_001_CHUNKS),
            ("002_intents", MIGRATION_002_INTENTS),
            ("003_eval_runs", MIGRATION_003_EVAL_RUNS),
            ("004_cognitive_memory", MIGRATION_004_COGNITIVE_MEMORY),
            ("005_semantic_cache", MIGRATION_005_SEMANTIC_CACHE),
            ("006_tool_events", MIGRATION_006_TOOL_EVENTS),
            ("007_worktree_scope", MIGRATION_007_WORKTREE_SCOPE),
            ("008_intent_locks", MIGRATION_008_INTENT_LOCKS),
            ("009_retrieval_weights", MIGRATION_009_RETRIEVAL_WEIGHTS),
        ];

        for (name, sql) in migrations {
            if !self.migration_applied(name).await? {
                debug!("Applying migration: {}", name);
                if *name == "007_worktree_scope" {
                    self.apply_migration_007_worktree_scope().await?;
                } else {
                    self.conn
                        .execute_batch(sql)
                        .await
                        .map_err(|e| HiefError::Migration(format!("{}: {}", name, e)))?;
                }

                self.conn
                    .execute("INSERT INTO _migrations (name) VALUES (?1)", [*name])
                    .await
                    .map_err(HiefError::Database)?;

                info!("Applied migration: {}", name);
            }
        }

        Ok(())
    }

    async fn apply_migration_007_worktree_scope(&self) -> Result<()> {
        if !self.table_has_column("chunk_access", "worktree_id").await? {
            self.conn
                .execute(
                    "ALTER TABLE chunk_access ADD COLUMN worktree_id TEXT NOT NULL DEFAULT 'project-root'",
                    (),
                )
                .await
                .map_err(|e| HiefError::Migration(format!("007_worktree_scope: {}", e)))?;
        }

        if !self.table_has_column("tool_events", "worktree_id").await? {
            self.conn
                .execute(
                    "ALTER TABLE tool_events ADD COLUMN worktree_id TEXT NOT NULL DEFAULT 'project-root'",
                    (),
                )
                .await
                .map_err(|e| HiefError::Migration(format!("007_worktree_scope: {}", e)))?;
        }

        self.conn
            .execute_batch(
                "CREATE INDEX IF NOT EXISTS idx_chunk_access_session_worktree
                    ON chunk_access(session_id, worktree_id);
                 CREATE INDEX IF NOT EXISTS idx_tool_events_session_worktree
                    ON tool_events(session_id, worktree_id);",
            )
            .await
            .map_err(|e| HiefError::Migration(format!("007_worktree_scope: {}", e)))?;

        Ok(())
    }

    async fn table_has_column(&self, table: &str, column: &str) -> Result<bool> {
        let pragma = format!("PRAGMA table_info({})", table);
        let mut rows = self
            .conn
            .query(&pragma, ())
            .await
            .map_err(HiefError::Database)?;

        while let Some(row) = rows.next().await.map_err(HiefError::Database)? {
            let name: String = row.get(1).map_err(HiefError::Database)?;
            if name == column {
                return Ok(true);
            }
        }

        Ok(false)
    }

    /// Check if a migration has already been applied.
    async fn migration_applied(&self, name: &str) -> Result<bool> {
        let mut rows = self
            .conn
            .query("SELECT 1 FROM _migrations WHERE name = ?1", [name])
            .await
            .map_err(HiefError::Database)?;

        Ok(rows.next().await.map_err(HiefError::Database)?.is_some())
    }

    /// Record a tool event for telemetry and observability.
    #[allow(dead_code)]
    pub async fn record_tool_event(
        &self,
        session_id: &str,
        tool: &str,
        query: &str,
        strategy: Option<&str>,
        result_count: Option<i32>,
        latency_ms: Option<i32>,
        groundedness_score: Option<f64>,
    ) -> Result<i64> {
        self.record_tool_event_scoped(
            session_id,
            tool,
            query,
            strategy,
            result_count,
            latency_ms,
            groundedness_score,
            None,
        )
        .await
    }

    /// Record a tool event scoped to a specific worktree.
    #[allow(dead_code)]
    pub async fn record_tool_event_scoped(
        &self,
        session_id: &str,
        tool: &str,
        query: &str,
        strategy: Option<&str>,
        result_count: Option<i32>,
        latency_ms: Option<i32>,
        groundedness_score: Option<f64>,
        worktree_id: Option<&str>,
    ) -> Result<i64> {
        use libsql::Value;

        let bounded_session_id = bounded_text(session_id, MAX_SESSION_ID_LEN);
        let bounded_tool = bounded_text(tool, MAX_TOOL_LEN);
        let bounded_query = bounded_text(query, MAX_QUERY_LEN);
        let bounded_strategy = strategy.map(|s| bounded_text(s, MAX_STRATEGY_LEN));
        let normalized_worktree_id = scope::normalize_worktree_id(worktree_id);

        let params = [
            Value::from(bounded_session_id),
            Value::from(bounded_tool),
            Value::from(bounded_query),
            bounded_strategy
                .as_deref()
                .map(Value::from)
                .unwrap_or(Value::Null),
            result_count.map(|v| Value::from(v as i64)).unwrap_or(Value::Null),
            latency_ms.map(|v| Value::from(v as i64)).unwrap_or(Value::Null),
            groundedness_score.map(Value::from).unwrap_or(Value::Null),
            Value::from(normalized_worktree_id),
        ];

        let event_id = self
            .conn
            .execute(
                r#"INSERT INTO tool_events
                   (session_id, tool, query, strategy, result_count, latency_ms, groundedness_score, worktree_id)
                   VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)"#,
                params,
            )
            .await
            .map_err(HiefError::Database)?;

        Ok(event_id as i64)
    }

    /// Query session summary metrics from telemetry data.
    #[allow(dead_code)]
    pub async fn get_session_summary(&self, session_id: &str) -> Result<Option<SessionSummary>> {
        self.get_session_summary_scoped(session_id, None).await
    }

    /// Query session summary metrics from telemetry data for a specific worktree.
    #[allow(dead_code)]
    pub async fn get_session_summary_scoped(
        &self,
        session_id: &str,
        worktree_id: Option<&str>,
    ) -> Result<Option<SessionSummary>> {
        let normalized_worktree_id = scope::normalize_worktree_id(worktree_id);
        let mut rows = self
            .conn
            .query(
                r#"SELECT
                    session_id,
                    COUNT(*) as total_events,
                    COUNT(DISTINCT tool) as unique_tools,
                    AVG(result_count) as avg_results,
                    AVG(latency_ms) as avg_latency_ms,
                    AVG(groundedness_score) as avg_groundedness,
                    MIN(created_at) as session_start,
                    MAX(created_at) as session_end,
                    MAX(created_at) - MIN(created_at) as session_duration_seconds
                   FROM tool_events
                   WHERE session_id = ?1 AND worktree_id = ?2
                   GROUP BY session_id"#,
                libsql::params![session_id, normalized_worktree_id.as_str()],
            )
            .await
            .map_err(HiefError::Database)?;

        if let Some(row) = rows.next().await.map_err(HiefError::Database)? {
            let summary = SessionSummary {
                session_id: row.get(0).map_err(HiefError::Database)?,
                total_events: row.get(1).map_err(HiefError::Database)?,
                unique_tools: row.get(2).map_err(HiefError::Database)?,
                avg_results: row.get(3).map_err(HiefError::Database)?,
                avg_latency_ms: row.get(4).map_err(HiefError::Database)?,
                avg_groundedness: row.get(5).map_err(HiefError::Database)?,
                session_start: row.get(6).map_err(HiefError::Database)?,
                session_end: row.get(7).map_err(HiefError::Database)?,
                session_duration_seconds: row.get(8).map_err(HiefError::Database)?,
            };
            Ok(Some(summary))
        } else {
            Ok(None)
        }
    }

    /// Query aggregate telemetry metrics with per-tool breakdown.
    pub async fn get_session_cost_summary(&self, session_id: &str) -> Result<SessionCostSummary> {
        self.get_session_cost_summary_scoped(session_id, None).await
    }

    /// Query aggregate telemetry metrics scoped to a specific worktree.
    pub async fn get_session_cost_summary_scoped(
        &self,
        session_id: &str,
        worktree_id: Option<&str>,
    ) -> Result<SessionCostSummary> {
        let normalized_worktree_id = scope::normalize_worktree_id(worktree_id);
        let mut totals_rows = self
            .conn
            .query(
                r#"SELECT
                    COUNT(*) as total_calls,
                    COALESCE(SUM(latency_ms), 0) as total_latency_ms,
                    AVG(groundedness_score) as avg_groundedness
                   FROM tool_events
                   WHERE session_id = ?1 AND worktree_id = ?2"#,
                libsql::params![session_id, normalized_worktree_id.as_str()],
            )
            .await
            .map_err(HiefError::Database)?;

        let (total_calls, total_latency_ms, avg_groundedness) =
            if let Some(row) = totals_rows.next().await.map_err(HiefError::Database)? {
                let total_calls: i64 = row.get(0).map_err(HiefError::Database)?;
                let total_latency_ms: i64 = row.get(1).map_err(HiefError::Database)?;
                let avg_groundedness: Option<f64> = row.get(2).map_err(HiefError::Database)?;
                (total_calls, total_latency_ms, avg_groundedness)
            } else {
                (0, 0, None)
            };

        let mut tool_rows = self
            .conn
            .query(
                r#"SELECT
                    tool,
                    COUNT(*) as calls,
                    COALESCE(SUM(latency_ms), 0) as total_latency_ms,
                    AVG(groundedness_score) as avg_groundedness
                   FROM tool_events
                         WHERE session_id = ?1 AND worktree_id = ?2
                   GROUP BY tool
                   ORDER BY calls DESC, tool ASC"#,
                    libsql::params![session_id, normalized_worktree_id.as_str()],
            )
            .await
            .map_err(HiefError::Database)?;

        let mut per_tool = Vec::new();
        while let Some(row) = tool_rows.next().await.map_err(HiefError::Database)? {
            per_tool.push(ToolBreakdown {
                tool: row.get(0).map_err(HiefError::Database)?,
                total_calls: row.get(1).map_err(HiefError::Database)?,
                total_latency_ms: row.get(2).map_err(HiefError::Database)?,
                avg_groundedness: row.get(3).map_err(HiefError::Database)?,
            });
        }

        Ok(SessionCostSummary {
            session_id: session_id.to_string(),
            total_calls,
            total_latency_ms,
            avg_groundedness,
            per_tool,
        })
    }

    /// Load latest retrieval weight snapshot row.
    pub async fn latest_retrieval_weight_snapshot(
        &self,
    ) -> Result<Option<RetrievalWeightSnapshotRow>> {
        let mut rows = self
            .conn
            .query(
                "SELECT id, current_json, candidate_json, applied, sample_size,
                        outcome_label, candidate_delta, learning_state,
                        last_learning_outcome, created_at
                 FROM retrieval_weight_snapshots
                 ORDER BY created_at DESC, id DESC
                 LIMIT 1",
                (),
            )
            .await
            .map_err(HiefError::Database)?;

        if let Some(row) = rows.next().await.map_err(HiefError::Database)? {
            Ok(Some(RetrievalWeightSnapshotRow {
                id: row.get(0).map_err(HiefError::Database)?,
                current_json: row.get(1).map_err(HiefError::Database)?,
                candidate_json: row.get::<String>(2).ok().filter(|v| !v.is_empty()),
                applied: row.get::<i64>(3).map_err(HiefError::Database)? != 0,
                sample_size: row.get(4).map_err(HiefError::Database)?,
                outcome_label: row.get(5).map_err(HiefError::Database)?,
                candidate_delta: row.get(6).ok(),
                learning_state: row.get(7).map_err(HiefError::Database)?,
                last_learning_outcome: row.get::<String>(8).ok().filter(|v| !v.is_empty()),
                created_at: row.get(9).map_err(HiefError::Database)?,
            }))
        } else {
            Ok(None)
        }
    }

    /// Persist retrieval weight snapshot metadata.
    pub async fn insert_retrieval_weight_snapshot(
        &self,
        snapshot: &RetrievalWeightSnapshotWrite,
    ) -> Result<i64> {
        let affected = self
            .conn
            .execute(
                "INSERT INTO retrieval_weight_snapshots
                 (current_json, candidate_json, applied, sample_size, outcome_label,
                  candidate_delta, learning_state, last_learning_outcome)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                libsql::params![
                    snapshot.current_json.as_str(),
                    snapshot.candidate_json.as_deref().unwrap_or(""),
                    if snapshot.applied { 1 } else { 0 },
                    snapshot.sample_size,
                    snapshot.outcome_label.as_str(),
                    snapshot.candidate_delta,
                    snapshot.learning_state.as_str(),
                    snapshot.last_learning_outcome.as_deref().unwrap_or(""),
                ],
            )
            .await
            .map_err(HiefError::Database)?;

        Ok(affected as i64)
    }

    /// Aggregate recent groundedness telemetry for bounded learning updates.
    pub async fn recent_groundedness_window(&self, limit: usize) -> Result<(i64, Option<f64>)> {
        let mut rows = self
            .conn
            .query(
                "WITH recent AS (
                    SELECT groundedness_score
                    FROM tool_events
                    WHERE groundedness_score IS NOT NULL
                    ORDER BY created_at DESC
                    LIMIT ?1
                 )
                 SELECT COUNT(*), AVG(groundedness_score) FROM recent",
                [limit as i64],
            )
            .await
            .map_err(HiefError::Database)?;

        if let Some(row) = rows.next().await.map_err(HiefError::Database)? {
            let count: i64 = row.get(0).map_err(HiefError::Database)?;
            let avg: Option<f64> = row.get(1).ok();
            Ok((count, avg))
        } else {
            Ok((0, None))
        }
    }
}

/// Session summary metrics aggregated from tool events.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct SessionSummary {
    pub session_id: String,
    pub total_events: i64,
    pub unique_tools: i64,
    pub avg_results: Option<f64>,
    pub avg_latency_ms: Option<f64>,
    pub avg_groundedness: Option<f64>,
    pub session_start: i64,
    pub session_end: i64,
    pub session_duration_seconds: i64,
}

/// Aggregate session telemetry with per-tool breakdown.
#[derive(Debug, Clone)]
pub struct SessionCostSummary {
    pub session_id: String,
    pub total_calls: i64,
    pub total_latency_ms: i64,
    pub avg_groundedness: Option<f64>,
    pub per_tool: Vec<ToolBreakdown>,
}

/// Per-tool telemetry totals for a session.
#[derive(Debug, Clone)]
pub struct ToolBreakdown {
    pub tool: String,
    pub total_calls: i64,
    pub total_latency_ms: i64,
    pub avg_groundedness: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetrievalWeightSnapshotRow {
    pub id: i64,
    pub current_json: String,
    pub candidate_json: Option<String>,
    pub applied: bool,
    pub sample_size: i64,
    pub outcome_label: String,
    pub candidate_delta: Option<f64>,
    pub learning_state: String,
    pub last_learning_outcome: Option<String>,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetrievalWeightSnapshotWrite {
    pub current_json: String,
    pub candidate_json: Option<String>,
    pub applied: bool,
    pub sample_size: i64,
    pub outcome_label: String,
    pub candidate_delta: Option<f64>,
    pub learning_state: String,
    pub last_learning_outcome: Option<String>,
}

fn bounded_text(input: &str, max_chars: usize) -> String {
    input.chars().take(max_chars).collect()
}

// ---------------------------------------------------------------------------
// Migration SQL
// ---------------------------------------------------------------------------

const MIGRATION_001_CHUNKS: &str = r#"
-- Code chunks: the core index
CREATE TABLE IF NOT EXISTS chunks (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    file_path TEXT NOT NULL,
    symbol_name TEXT,
    symbol_kind TEXT,
    parent_scope TEXT,
    language TEXT NOT NULL,
    content TEXT NOT NULL,
    start_line INTEGER NOT NULL,
    end_line INTEGER NOT NULL,
    content_hash TEXT NOT NULL,
    indexed_at INTEGER DEFAULT (unixepoch())
);

CREATE INDEX IF NOT EXISTS idx_chunks_file ON chunks(file_path);
CREATE INDEX IF NOT EXISTS idx_chunks_kind ON chunks(symbol_kind);
CREATE INDEX IF NOT EXISTS idx_chunks_lang ON chunks(language);

-- FTS5 for fast keyword search
CREATE VIRTUAL TABLE IF NOT EXISTS chunks_fts USING fts5(
    file_path,
    symbol_name,
    content,
    symbol_kind,
    content='chunks',
    content_rowid='id',
    tokenize='porter unicode61'
);

-- Triggers to keep FTS5 in sync
CREATE TRIGGER IF NOT EXISTS chunks_ai AFTER INSERT ON chunks BEGIN
    INSERT INTO chunks_fts(rowid, file_path, symbol_name, content, symbol_kind)
    VALUES (new.id, new.file_path, new.symbol_name, new.content, new.symbol_kind);
END;

CREATE TRIGGER IF NOT EXISTS chunks_ad AFTER DELETE ON chunks BEGIN
    INSERT INTO chunks_fts(chunks_fts, rowid, file_path, symbol_name, content, symbol_kind)
    VALUES ('delete', old.id, old.file_path, old.symbol_name, old.content, old.symbol_kind);
END;

-- File metadata for incremental indexing
CREATE TABLE IF NOT EXISTS file_meta (
    file_path TEXT PRIMARY KEY,
    content_hash TEXT NOT NULL,
    chunk_count INTEGER NOT NULL,
    language TEXT,
    indexed_at INTEGER DEFAULT (unixepoch())
);
"#;

const MIGRATION_002_INTENTS: &str = r#"
-- Intent nodes: units of work
CREATE TABLE IF NOT EXISTS intents (
    id TEXT PRIMARY KEY,
    kind TEXT NOT NULL
        CHECK(kind IN ('feature','bug','refactor','spike','test','chore')),
    title TEXT NOT NULL,
    description TEXT,
    status TEXT NOT NULL DEFAULT 'draft'
        CHECK(status IN ('draft','approved','in_progress','in_review',
                          'verified','merged','rejected','blocked')),
    priority TEXT NOT NULL DEFAULT 'medium'
        CHECK(priority IN ('critical','high','medium','low')),
    criteria TEXT,
    labels TEXT,
    assigned_to TEXT,
    created_at INTEGER DEFAULT (unixepoch()),
    updated_at INTEGER DEFAULT (unixepoch())
);

CREATE INDEX IF NOT EXISTS idx_intents_status ON intents(status);
CREATE INDEX IF NOT EXISTS idx_intents_kind ON intents(kind);

-- Dependency edges between intents
CREATE TABLE IF NOT EXISTS intent_edges (
    from_id TEXT NOT NULL REFERENCES intents(id) ON DELETE CASCADE,
    to_id TEXT NOT NULL REFERENCES intents(id) ON DELETE CASCADE,
    kind TEXT NOT NULL DEFAULT 'depends_on'
        CHECK(kind IN ('depends_on','blocks','implements','tests','related_to')),
    created_at INTEGER DEFAULT (unixepoch()),
    PRIMARY KEY (from_id, to_id, kind),
    CHECK(from_id != to_id)
);
"#;

const MIGRATION_003_EVAL_RUNS: &str = r#"
CREATE TABLE IF NOT EXISTS eval_runs (
    id TEXT PRIMARY KEY,
    golden_set TEXT NOT NULL,
    overall_score REAL NOT NULL,
    passed BOOLEAN NOT NULL,
    details TEXT,
    git_commit TEXT,
    created_at INTEGER DEFAULT (unixepoch())
);

CREATE INDEX IF NOT EXISTS idx_eval_set ON eval_runs(golden_set, created_at);
"#;

const MIGRATION_004_COGNITIVE_MEMORY: &str = r#"
-- Access tracking: every time an agent retrieves a code chunk
CREATE TABLE IF NOT EXISTS chunk_access (
    id          TEXT PRIMARY KEY,
    chunk_id    TEXT,
    file_path   TEXT NOT NULL,
    query       TEXT,
    tool        TEXT NOT NULL,
    session_id  TEXT,
    accessed_at INTEGER NOT NULL DEFAULT (unixepoch())
);

CREATE INDEX IF NOT EXISTS idx_chunk_access_file ON chunk_access(file_path);
CREATE INDEX IF NOT EXISTS idx_chunk_access_time ON chunk_access(accessed_at);
CREATE INDEX IF NOT EXISTS idx_chunk_access_session ON chunk_access(session_id);

-- Co-access graph: Hebbian co-activation tracking
CREATE TABLE IF NOT EXISTS co_access (
    file_a          TEXT NOT NULL,
    file_b          TEXT NOT NULL,
    strength        REAL NOT NULL DEFAULT 1.0,
    last_co_access  INTEGER NOT NULL DEFAULT (unixepoch()),
    PRIMARY KEY (file_a, file_b)
);

CREATE INDEX IF NOT EXISTS idx_co_access_strength ON co_access(strength DESC);
"#;

const MIGRATION_005_SEMANTIC_CACHE: &str = r#"
-- Semantic cache for repeated retrieval queries
CREATE TABLE IF NOT EXISTS semantic_cache (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    query_fingerprint TEXT NOT NULL,
    embedding_hash TEXT NOT NULL,
    language_scope TEXT NOT NULL,
    result_payload_hash TEXT NOT NULL,
    result_json TEXT NOT NULL,
    expires_at INTEGER NOT NULL,
    created_at INTEGER NOT NULL DEFAULT (unixepoch())
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_semantic_cache_key
    ON semantic_cache(query_fingerprint, embedding_hash, language_scope);
CREATE INDEX IF NOT EXISTS idx_semantic_cache_expires_at
    ON semantic_cache(expires_at);
"#;

const MIGRATION_006_TOOL_EVENTS: &str = r#"
-- Tool invocation telemetry for observability and eval feedback
CREATE TABLE IF NOT EXISTS tool_events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT NOT NULL,
    tool TEXT NOT NULL,
    query TEXT NOT NULL,
    strategy TEXT,
    result_count INTEGER,
    latency_ms INTEGER,
    groundedness_score REAL,
    created_at INTEGER NOT NULL DEFAULT (unixepoch())
);

CREATE INDEX IF NOT EXISTS idx_tool_events_session ON tool_events(session_id);
CREATE INDEX IF NOT EXISTS idx_tool_events_tool ON tool_events(tool);
CREATE INDEX IF NOT EXISTS idx_tool_events_created ON tool_events(created_at);

-- Session summary view: aggregate metrics per session
CREATE VIEW IF NOT EXISTS session_summary AS
SELECT
    session_id,
    COUNT(*) as total_events,
    COUNT(DISTINCT tool) as unique_tools,
    AVG(result_count) as avg_results,
    AVG(latency_ms) as avg_latency_ms,
    AVG(groundedness_score) as avg_groundedness,
    MIN(created_at) as session_start,
    MAX(created_at) as session_end,
    MAX(created_at) - MIN(created_at) as session_duration_seconds
FROM tool_events
GROUP BY session_id;
"#;

const MIGRATION_007_WORKTREE_SCOPE: &str = r#"
-- handled by apply_migration_007_worktree_scope() for replay-safe ALTER TABLE
"#;

const MIGRATION_008_INTENT_LOCKS: &str = r#"
CREATE TABLE IF NOT EXISTS intent_locks (
    intent_id TEXT PRIMARY KEY REFERENCES intents(id) ON DELETE CASCADE,
    holder TEXT NOT NULL,
    worktree_id TEXT NOT NULL,
    acquired_at INTEGER NOT NULL DEFAULT (unixepoch()),
    expires_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_intent_locks_expires ON intent_locks(expires_at);
CREATE INDEX IF NOT EXISTS idx_intent_locks_worktree ON intent_locks(worktree_id);
"#;

const MIGRATION_009_RETRIEVAL_WEIGHTS: &str = r#"
CREATE TABLE IF NOT EXISTS retrieval_weight_snapshots (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    current_json TEXT NOT NULL,
    candidate_json TEXT,
    applied INTEGER NOT NULL DEFAULT 0,
    sample_size INTEGER NOT NULL DEFAULT 0,
    outcome_label TEXT NOT NULL DEFAULT 'neutral',
    candidate_delta REAL,
    learning_state TEXT NOT NULL DEFAULT 'neutral',
    last_learning_outcome TEXT,
    created_at INTEGER NOT NULL DEFAULT (unixepoch())
);

CREATE INDEX IF NOT EXISTS idx_retrieval_weight_snapshots_created
    ON retrieval_weight_snapshots(created_at DESC);
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_open_memory_database() {
        let db = Database::open_memory().await;
        assert!(db.is_ok(), "Should create in-memory database");
    }

    #[tokio::test]
    async fn test_migrations_applied() {
        let db = Database::open_memory().await.unwrap();

        // Verify the migrations table exists and has entries
        let mut rows = db
            .conn()
            .query("SELECT name FROM _migrations ORDER BY id", ())
            .await
            .unwrap();

        let mut names = Vec::new();
        while let Some(row) = rows.next().await.unwrap() {
            let name: String = row.get(0).unwrap();
            names.push(name);
        }

        assert_eq!(
            names,
            vec![
                "001_chunks",
                "002_intents",
                "003_eval_runs",
                "004_cognitive_memory",
                "005_semantic_cache",
                "006_tool_events",
                "007_worktree_scope",
                "008_intent_locks",
                "009_retrieval_weights"
            ]
        );
    }

    #[tokio::test]
    async fn test_chunks_table_exists() {
        let db = Database::open_memory().await.unwrap();

        // Insert a chunk
        db.conn()
            .execute(
                "INSERT INTO chunks (file_path, language, content, start_line, end_line, content_hash)
                 VALUES ('test.rs', 'rust', 'fn main() {}', 0, 0, 'abc123')",
                (),
            )
            .await
            .unwrap();

        // Verify it was inserted
        let mut rows = db
            .conn()
            .query("SELECT COUNT(*) FROM chunks", ())
            .await
            .unwrap();
        let row = rows.next().await.unwrap().unwrap();
        let count: i64 = row.get(0).unwrap();
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn test_fts5_table_exists() {
        let db = Database::open_memory().await.unwrap();

        // Insert a chunk (triggers FTS sync)
        db.conn()
            .execute(
                "INSERT INTO chunks (file_path, symbol_name, language, content, start_line, end_line, content_hash)
                 VALUES ('test.rs', 'main', 'rust', 'fn main() { println!(\"hello\"); }', 0, 0, 'abc')",
                (),
            )
            .await
            .unwrap();

        // Search via FTS5
        let mut rows = db
            .conn()
            .query(
                "SELECT file_path FROM chunks_fts WHERE chunks_fts MATCH 'hello'",
                (),
            )
            .await
            .unwrap();

        let row = rows.next().await.unwrap();
        assert!(row.is_some(), "FTS5 should find the content");
    }

    #[tokio::test]
    async fn test_intents_table_exists() {
        let db = Database::open_memory().await.unwrap();

        db.conn()
            .execute(
                "INSERT INTO intents (id, kind, title) VALUES ('test-id', 'feature', 'Test')",
                (),
            )
            .await
            .unwrap();

        let mut rows = db
            .conn()
            .query("SELECT title FROM intents WHERE id = 'test-id'", ())
            .await
            .unwrap();
        let row = rows.next().await.unwrap().unwrap();
        let title: String = row.get(0).unwrap();
        assert_eq!(title, "Test");
    }

    #[tokio::test]
    async fn test_eval_runs_table_exists() {
        let db = Database::open_memory().await.unwrap();

        db.conn()
            .execute(
                "INSERT INTO eval_runs (id, golden_set, overall_score, passed)
                 VALUES ('run-1', 'basic', 0.95, 1)",
                (),
            )
            .await
            .unwrap();

        let mut rows = db
            .conn()
            .query("SELECT overall_score FROM eval_runs WHERE id = 'run-1'", ())
            .await
            .unwrap();
        let row = rows.next().await.unwrap().unwrap();
        let score: f64 = row.get(0).unwrap();
        assert!((score - 0.95).abs() < f64::EPSILON);
    }

    #[tokio::test]
    async fn test_file_meta_table() {
        let db = Database::open_memory().await.unwrap();

        db.conn()
            .execute(
                "INSERT INTO file_meta (file_path, content_hash, chunk_count, language)
                 VALUES ('src/main.rs', 'deadbeef', 5, 'rust')",
                (),
            )
            .await
            .unwrap();

        let mut rows = db
            .conn()
            .query(
                "SELECT chunk_count FROM file_meta WHERE file_path = 'src/main.rs'",
                (),
            )
            .await
            .unwrap();
        let row = rows.next().await.unwrap().unwrap();
        let count: i64 = row.get(0).unwrap();
        assert_eq!(count, 5);
    }

    #[tokio::test]
    async fn test_open_file_database() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");

        let db = Database::open(&db_path).await;
        assert!(db.is_ok(), "Should create file-based database");
        assert!(db_path.exists(), "Database file should exist");
    }

    #[tokio::test]
    async fn test_migrations_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");

        // Open twice — migrations should not fail on second open
        {
            let _db = Database::open(&db_path).await.unwrap();
        }
        {
            let db = Database::open(&db_path).await.unwrap();
            // Verify still works
            let mut rows = db
                .conn()
                .query("SELECT COUNT(*) FROM _migrations", ())
                .await
                .unwrap();
            let row = rows.next().await.unwrap().unwrap();
            let count: i64 = row.get(0).unwrap();
            assert_eq!(count, 9, "Expected 9 migrations (001_chunks, 002_intents, 003_eval_runs, 004_cognitive_memory, 005_semantic_cache, 006_tool_events, 007_worktree_scope, 008_intent_locks, 009_retrieval_weights)");
        }
    }

    #[tokio::test]
    async fn test_foreign_keys_enabled() {
        let db = Database::open_memory().await.unwrap();

        // Create an intent
        db.conn()
            .execute(
                "INSERT INTO intents (id, kind, title) VALUES ('i1', 'feature', 'Test')",
                (),
            )
            .await
            .unwrap();

        // Try to insert an edge referencing a non-existent intent
        let result = db
            .conn()
            .execute(
                "INSERT INTO intent_edges (from_id, to_id, kind) VALUES ('i1', 'nonexistent', 'depends_on')",
                (),
            )
            .await;

        assert!(
            result.is_err(),
            "Foreign key constraint should prevent invalid references"
        );
    }
}
