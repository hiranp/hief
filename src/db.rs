//! Database connection and migrations for libsql.

use libsql::{Builder, Connection};
use std::path::Path;
use tracing::{info, debug};

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
        let _ = conn.query("PRAGMA journal_mode=WAL", ()).await.map_err(HiefError::Database)?;
        let _ = conn.execute("PRAGMA synchronous=NORMAL", ()).await.map_err(HiefError::Database)?;
        let _ = conn.query("PRAGMA foreign_keys=ON", ()).await.map_err(HiefError::Database)?;

        let database = Self { conn };
        database.run_migrations().await?;
        Ok(database)
    }

    /// Open an in-memory database (for testing).
    pub async fn open_memory() -> Result<Self> {
        let db = Builder::new_local(":memory:")
            .build()
            .await
            .map_err(HiefError::Database)?;

        let conn = db.connect().map_err(HiefError::Database)?;
        conn.execute("PRAGMA foreign_keys=ON", ()).await.map_err(HiefError::Database)?;

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
        ];

        for (name, sql) in migrations {
            if !self.migration_applied(name).await? {
                debug!("Applying migration: {}", name);
                self.conn
                    .execute_batch(sql)
                    .await
                    .map_err(|e| HiefError::Migration(format!("{}: {}", name, e)))?;

                self.conn
                    .execute(
                        "INSERT INTO _migrations (name) VALUES (?1)",
                        [*name],
                    )
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

        assert_eq!(names, vec!["001_chunks", "002_intents", "003_eval_runs"]);
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
            .query("SELECT file_path FROM chunks_fts WHERE chunks_fts MATCH 'hello'", ())
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
            .query("SELECT chunk_count FROM file_meta WHERE file_path = 'src/main.rs'", ())
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
            assert_eq!(count, 3);
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

        assert!(result.is_err(), "Foreign key constraint should prevent invalid references");
    }
}
