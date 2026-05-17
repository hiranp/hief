//! Database connection and migrations for libsql.

use libsql::{Builder, Connection};
use std::path::Path;
use tracing::{debug, info};

use crate::errors::{HiefError, Result};

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
        ];

        for (name, sql) in migrations {
            if !self.migration_applied(name).await? {
                debug!("Applying migration: {}", name);
                self.conn
                    .execute_batch(sql)
                    .await
                    .map_err(|e| HiefError::Migration(format!("{}: {}", name, e)))?;

                self.conn
                    .execute("INSERT INTO _migrations (name) VALUES (?1)", [*name])
                    .await
                    .map_err(HiefError::Database)?;

                info!("Applied migration: {}", name);
            }
        }

        Ok(())
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
        use libsql::Value;
        
        let params = [
            Value::from(session_id),
            Value::from(tool),
            Value::from(query),
            strategy.map(Value::from).unwrap_or(Value::Null),
            result_count.map(|v| Value::from(v as i64)).unwrap_or(Value::Null),
            latency_ms.map(|v| Value::from(v as i64)).unwrap_or(Value::Null),
            groundedness_score.map(Value::from).unwrap_or(Value::Null),
        ];

        let event_id = self
            .conn
            .execute(
                r#"INSERT INTO tool_events
                   (session_id, tool, query, strategy, result_count, latency_ms, groundedness_score)
                   VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)"#,
                params,
            )
            .await
            .map_err(HiefError::Database)?;

        Ok(event_id as i64)
    }

    /// Query session summary metrics from telemetry data.
    #[allow(dead_code)]
    pub async fn get_session_summary(&self, session_id: &str) -> Result<Option<SessionSummary>> {
        let mut rows = self
            .conn
            .query(
                r#"SELECT
                    session_id,
                    total_events,
                    unique_tools,
                    avg_results,
                    avg_latency_ms,
                    avg_groundedness,
                    session_start,
                    session_end,
                    session_duration_seconds
                   FROM session_summary
                   WHERE session_id = ?1"#,
                [session_id],
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
                "006_tool_events"
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
            assert_eq!(count, 6, "Expected 6 migrations (001_chunks, 002_intents, 003_eval_runs, 004_cognitive_memory, 005_semantic_cache, 006_tool_events)");
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
