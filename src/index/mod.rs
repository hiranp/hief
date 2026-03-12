//! Code indexing: AST-aware chunking, file walking, and FTS5 search.

pub mod chunker;
pub mod memory;
pub mod search;
pub mod structural;
pub mod vectors;
pub mod walker;

use std::collections::HashMap;
use std::path::Path;
use tracing::{info, warn};

use crate::config::IndexConfig;
use crate::db::Database;
use crate::errors::Result;

use schemars::JsonSchema;

use self::chunker::{Chunk, Chunker};
use self::search::{SearchQuery, SearchResult};
use self::walker::FileWalker;

/// Statistics about the current index state.
#[derive(Debug, Clone, serde::Serialize, JsonSchema)]
pub struct IndexStats {
    pub total_files: usize,
    pub total_chunks: usize,
    pub languages: HashMap<String, usize>,
    pub last_indexed: Option<i64>,
    pub db_size_bytes: u64,
}

/// Result of an incremental index build.
#[derive(Debug, Clone, serde::Serialize)]
pub struct BuildReport {
    pub files_new: usize,
    pub files_updated: usize,
    pub files_removed: usize,
    pub total_chunks: usize,
    pub duration_ms: u64,
}

/// Build or update the code index incrementally.
pub async fn build(
    db: &Database,
    project_root: &Path,
    config: &IndexConfig,
) -> Result<BuildReport> {
    let start = std::time::Instant::now();
    let mut files_new = 0usize;
    let mut files_updated = 0usize;
    let mut total_chunks = 0usize;

    let walker = FileWalker::new(project_root);
    let chunker = Chunker::new(config.max_chunk_tokens);
    let files = walker.walk()?;

    // Track which files we've seen for pruning
    let mut seen_paths: Vec<String> = Vec::with_capacity(files.len());

    for file_entry in &files {
        let rel_path = file_entry.rel_path.clone();
        seen_paths.push(rel_path.clone());

        let content = match std::fs::read_to_string(&file_entry.abs_path) {
            Ok(c) => c,
            Err(e) => {
                warn!("Skipping {}: {}", rel_path, e);
                continue;
            }
        };

        let file_hash = blake3::hash(content.as_bytes()).to_hex().to_string();

        // Check if file is already indexed with the same hash
        let existing_hash = get_file_hash(db, &rel_path).await?;
        if existing_hash.as_deref() == Some(file_hash.as_str()) {
            // File unchanged, skip
            let count = get_chunk_count(db, &rel_path).await?;
            total_chunks += count;
            continue;
        }

        // Determine language
        let language = match file_entry.language.as_deref() {
            Some(l) => l.to_string(),
            None => {
                warn!("Skipping {} (unknown language)", rel_path);
                continue;
            }
        };

        // Delete old chunks for this file
        if existing_hash.is_some() {
            delete_file_chunks(db, &rel_path).await?;
            files_updated += 1;
        } else {
            files_new += 1;
        }

        // Parse and chunk
        let chunks = chunker.chunk(&content, &language, &rel_path);

        // Insert chunks
        let chunk_count = chunks.len();
        for chunk in &chunks {
            insert_chunk(db, chunk).await?;
        }

        // Update file metadata
        upsert_file_meta(db, &rel_path, &file_hash, chunk_count, &language).await?;
        total_chunks += chunk_count;
    }

    // Prune files no longer on disk
    let files_removed = prune_deleted_files(db, &seen_paths).await?;

    let duration_ms = start.elapsed().as_millis() as u64;

    let report = BuildReport {
        files_new,
        files_updated,
        files_removed,
        total_chunks,
        duration_ms,
    };

    info!(
        "Indexed {} files ({} new, {} updated, {} removed), {} chunks, {}ms",
        files_new + files_updated,
        files_new,
        files_updated,
        files_removed,
        total_chunks,
        duration_ms,
    );

    Ok(report)
}

/// Run a search query against the index.
pub async fn search(db: &Database, query: &SearchQuery) -> Result<Vec<SearchResult>> {
    search::search(db, query).await
}

/// Get index statistics.
pub async fn status(db: &Database, project_root: &Path) -> Result<IndexStats> {
    let conn = db.conn();

    let mut rows = conn
        .query("SELECT COUNT(*) FROM chunks", ())
        .await
        .map_err(crate::errors::HiefError::Database)?;
    let total_chunks: usize = if let Some(row) = rows
        .next()
        .await
        .map_err(crate::errors::HiefError::Database)?
    {
        row.get::<i64>(0)
            .map_err(crate::errors::HiefError::Database)? as usize
    } else {
        0
    };

    let mut rows = conn
        .query("SELECT COUNT(*) FROM file_meta", ())
        .await
        .map_err(crate::errors::HiefError::Database)?;
    let total_files: usize = if let Some(row) = rows
        .next()
        .await
        .map_err(crate::errors::HiefError::Database)?
    {
        row.get::<i64>(0)
            .map_err(crate::errors::HiefError::Database)? as usize
    } else {
        0
    };

    let mut languages = HashMap::new();
    let mut rows = conn
        .query(
            "SELECT language, COUNT(*) as cnt FROM file_meta GROUP BY language",
            (),
        )
        .await
        .map_err(crate::errors::HiefError::Database)?;
    while let Some(row) = rows
        .next()
        .await
        .map_err(crate::errors::HiefError::Database)?
    {
        let lang: String = row.get(0).map_err(crate::errors::HiefError::Database)?;
        let count: i64 = row.get(1).map_err(crate::errors::HiefError::Database)?;
        languages.insert(lang, count as usize);
    }

    let mut rows = conn
        .query("SELECT MAX(indexed_at) FROM file_meta", ())
        .await
        .map_err(crate::errors::HiefError::Database)?;
    let last_indexed: Option<i64> = if let Some(row) = rows
        .next()
        .await
        .map_err(crate::errors::HiefError::Database)?
    {
        row.get(0).ok()
    } else {
        None
    };

    // Get DB file size
    let db_path = crate::config::Config::db_path(project_root);
    let db_size_bytes = std::fs::metadata(&db_path).map(|m| m.len()).unwrap_or(0);

    Ok(IndexStats {
        total_files,
        total_chunks,
        languages,
        last_indexed,
        db_size_bytes,
    })
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

async fn get_file_hash(db: &Database, file_path: &str) -> Result<Option<String>> {
    let mut rows = db
        .conn()
        .query(
            "SELECT content_hash FROM file_meta WHERE file_path = ?1",
            [file_path],
        )
        .await
        .map_err(crate::errors::HiefError::Database)?;
    if let Some(row) = rows
        .next()
        .await
        .map_err(crate::errors::HiefError::Database)?
    {
        Ok(Some(
            row.get(0).map_err(crate::errors::HiefError::Database)?,
        ))
    } else {
        Ok(None)
    }
}

async fn get_chunk_count(db: &Database, file_path: &str) -> Result<usize> {
    let mut rows = db
        .conn()
        .query(
            "SELECT chunk_count FROM file_meta WHERE file_path = ?1",
            [file_path],
        )
        .await
        .map_err(crate::errors::HiefError::Database)?;
    if let Some(row) = rows
        .next()
        .await
        .map_err(crate::errors::HiefError::Database)?
    {
        Ok(row
            .get::<i64>(0)
            .map_err(crate::errors::HiefError::Database)? as usize)
    } else {
        Ok(0)
    }
}

async fn delete_file_chunks(db: &Database, file_path: &str) -> Result<()> {
    db.conn()
        .execute("DELETE FROM chunks WHERE file_path = ?1", [file_path])
        .await
        .map_err(crate::errors::HiefError::Database)?;
    Ok(())
}

async fn insert_chunk(db: &Database, chunk: &Chunk) -> Result<()> {
    db.conn()
        .execute(
            "INSERT INTO chunks (file_path, symbol_name, symbol_kind, parent_scope, language, content, start_line, end_line, content_hash)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            libsql::params![
                chunk.file_path.as_str(),
                chunk.symbol_name.as_deref().unwrap_or(""),
                chunk.symbol_kind.as_deref().unwrap_or(""),
                chunk.parent_scope.as_deref().unwrap_or(""),
                chunk.language.as_str(),
                chunk.content.as_str(),
                chunk.start_line as i64,
                chunk.end_line as i64,
                chunk.content_hash.as_str(),
            ],
        )
        .await
        .map_err(crate::errors::HiefError::Database)?;
    Ok(())
}

async fn upsert_file_meta(
    db: &Database,
    file_path: &str,
    content_hash: &str,
    chunk_count: usize,
    language: &str,
) -> Result<()> {
    db.conn()
        .execute(
            "INSERT OR REPLACE INTO file_meta (file_path, content_hash, chunk_count, language)
             VALUES (?1, ?2, ?3, ?4)",
            libsql::params![file_path, content_hash, chunk_count as i64, language],
        )
        .await
        .map_err(crate::errors::HiefError::Database)?;
    Ok(())
}

async fn prune_deleted_files(db: &Database, seen_paths: &[String]) -> Result<usize> {
    // Get all indexed file paths
    let mut rows = db
        .conn()
        .query("SELECT file_path FROM file_meta", ())
        .await
        .map_err(crate::errors::HiefError::Database)?;

    let mut to_delete = Vec::new();
    while let Some(row) = rows
        .next()
        .await
        .map_err(crate::errors::HiefError::Database)?
    {
        let path: String = row.get(0).map_err(crate::errors::HiefError::Database)?;
        if !seen_paths.contains(&path) {
            to_delete.push(path);
        }
    }

    for path in &to_delete {
        delete_file_chunks(db, path).await?;
        db.conn()
            .execute(
                "DELETE FROM file_meta WHERE file_path = ?1",
                [path.as_str()],
            )
            .await
            .map_err(crate::errors::HiefError::Database)?;
    }

    Ok(to_delete.len())
}
