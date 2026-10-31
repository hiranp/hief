//! Semantic vector search, stored in the same libsql database as the index.
//!
//! This module provides vector storage (`F32_BLOB` column, `vector_distance_*`) and similarity search for code
//! chunks. It complements the FTS5 keyword search in [`search`] and the
//! structural pattern search in [`structural`].
//!
//! HIEF does not bundle an LLM. For this milestone, embeddings are generated
//! locally with a deterministic hashing-based encoder so semantic search is
//! self-contained and testable without any external model service.

#![allow(dead_code)]

use std::time::{SystemTime, UNIX_EPOCH};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tracing::info;

use crate::db::Database;
use crate::errors::{HiefError, Result};

use super::chunker::Chunk;

const TABLE_NAME: &str = "code_vectors";
const SEMANTIC_CACHE_TTL_SECS: i64 = 30 * 60;

/// Configuration for the vector search subsystem.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorConfig {
    /// Whether vector search is enabled (default: false).
    #[serde(default)]
    pub enabled: bool,
    /// Embedding dimensions (default: 384 for small models, 768/1536 for larger).
    #[serde(default = "default_dimensions")]
    pub dimensions: usize,
    /// Distance metric: "cosine" (default) or "l2".
    #[serde(default = "default_metric")]
    pub metric: String,
}

impl Default for VectorConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            dimensions: default_dimensions(),
            metric: default_metric(),
        }
    }
}

fn default_dimensions() -> usize {
    384
}

fn default_metric() -> String {
    "cosine".to_string()
}

/// A code chunk with its embedding vector.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddedChunk {
    /// Stable chunk identifier derived from indexed chunk metadata.
    pub chunk_id: String,
    /// File path relative to project root.
    pub file_path: String,
    /// Symbol name (if extracted from AST).
    pub symbol_name: Option<String>,
    /// Parent scope if available.
    pub parent_scope: Option<String>,
    /// Language of the source file.
    pub language: String,
    /// Raw chunk content for snippet/result output.
    pub content: String,
    /// Start line of the chunk in the source file.
    pub start_line: u32,
    /// End line of the chunk in the source file.
    pub end_line: u32,
    /// The embedding vector.
    pub vector: Vec<f32>,
}

/// Result of a semantic search query.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SemanticResult {
    /// Chunk ID.
    pub chunk_id: String,
    /// File path relative to project root.
    pub file_path: String,
    /// Symbol name if available.
    pub symbol_name: Option<String>,
    /// Parent scope if available.
    pub parent_scope: Option<String>,
    /// Language.
    pub language: String,
    /// Code content of the chunk.
    pub content: String,
    /// Start line of the chunk in the source file.
    pub start_line: u32,
    /// End line of the chunk in the source file.
    pub end_line: u32,
    /// Similarity score (0.0 - 1.0, higher is more similar).
    pub score: f32,
}

/// Semantic search query parameters.
#[derive(Debug, Clone)]
pub struct SemanticQuery {
    /// The query text (will be embedded before searching).
    pub query: String,
    /// Max results to return.
    pub top_k: usize,
    /// Optional language filter.
    pub language: Option<String>,
}

impl SemanticQuery {
    pub fn new(query: &str) -> Self {
        Self {
            query: query.to_string(),
            top_k: 10,
            language: None,
        }
    }
}

/// Metadata returned alongside semantic search results.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SemanticSearchOutcome {
    pub results: Vec<SemanticResult>,
    pub cache_used: bool,
    pub groundedness_score: Option<f64>,
}

/// Convert indexed chunks into deterministic embeddings for storage.
pub fn embed_chunks(chunks: &[Chunk], dimensions: usize) -> Result<Vec<EmbeddedChunk>> {
    chunks
        .iter()
        .map(|chunk| {
            Ok(EmbeddedChunk {
                chunk_id: chunk_id(chunk),
                file_path: chunk.file_path.clone(),
                symbol_name: chunk.symbol_name.clone(),
                parent_scope: chunk.parent_scope.clone(),
                language: chunk.language.clone(),
                content: chunk.content.clone(),
                start_line: chunk.start_line,
                end_line: chunk.end_line,
                vector: embed_text(&chunk.content, dimensions)?,
            })
        })
        .collect()
}

/// Deterministically embed a text string into a fixed-size vector.
pub fn embed_text(text: &str, dimensions: usize) -> Result<Vec<f32>> {
    if dimensions == 0 {
        return Err(HiefError::Config(
            "vectors.dimensions must be greater than zero".to_string(),
        ));
    }

    let mut vector = vec![0.0_f32; dimensions];
    let normalized = normalize_text(text);
    let tokens = tokenize(&normalized);

    if tokens.is_empty() {
        vector[0] = 1.0;
        return Ok(vector);
    }

    for token in &tokens {
        accumulate_feature(
            &mut vector,
            token,
            1.0 + (token.len().min(12) as f32 / 12.0),
        );
    }

    for trigram in char_ngrams(&normalized, 3) {
        accumulate_feature(&mut vector, &trigram, 0.35);
    }

    normalize_vector(&mut vector);
    Ok(vector)
}

/// Create the vector table if it does not exist.
///
/// The table lives in the main database, so the column width is fixed here
/// (from `vectors.dimensions`) instead of in a static migration. Returns
/// `true` when the table was just created: the caller must then embed every
/// file, even the unchanged ones.
pub async fn init(db: &Database, config: &VectorConfig) -> Result<bool> {
    if !config.enabled {
        info!("Vector search is disabled in configuration");
        return Ok(false);
    }
    distance_function(&config.metric)?;
    if config.dimensions == 0 {
        return Err(HiefError::Config(
            "vectors.dimensions must be greater than zero".to_string(),
        ));
    }

    let existed = table_exists(db).await?;
    if !existed {
        db.conn()
            .execute(
                &format!(
                    "CREATE TABLE {TABLE_NAME} (
                        chunk_id TEXT PRIMARY KEY,
                        file_path TEXT NOT NULL,
                        symbol_name TEXT,
                        parent_scope TEXT,
                        language TEXT NOT NULL,
                        content TEXT NOT NULL,
                        start_line INTEGER NOT NULL,
                        end_line INTEGER NOT NULL,
                        embedding F32_BLOB({})
                    )",
                    config.dimensions
                ),
                (),
            )
            .await
            .map_err(HiefError::Database)?;
        db.conn()
            .execute(
                &format!("CREATE INDEX idx_{TABLE_NAME}_file ON {TABLE_NAME}(file_path)"),
                (),
            )
            .await
            .map_err(HiefError::Database)?;
        info!(
            "Vector store initialized (dimensions={}, metric={})",
            config.dimensions, config.metric
        );
    }
    Ok(!existed)
}

/// Whether the vector table exists in this database.
pub async fn table_exists(db: &Database) -> Result<bool> {
    let mut rows = db
        .conn()
        .query(
            "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1",
            libsql::params![TABLE_NAME],
        )
        .await
        .map_err(HiefError::Database)?;
    Ok(rows.next().await.map_err(HiefError::Database)?.is_some())
}

/// Store embeddings for a batch of code chunks (replacing any with the same id).
pub async fn store_embeddings(
    db: &Database,
    chunks: &[EmbeddedChunk],
    config: &VectorConfig,
) -> Result<usize> {
    if chunks.is_empty() {
        return Ok(0);
    }

    validate_dimensions(chunks, config.dimensions)?;
    init(db, config).await?;

    let conn = db.conn();
    conn.execute("BEGIN", ())
        .await
        .map_err(HiefError::Database)?;
    for chunk in chunks {
        let result = conn
            .execute(
                &format!(
                    "INSERT OR REPLACE INTO {TABLE_NAME}
                     (chunk_id, file_path, symbol_name, parent_scope, language,
                      content, start_line, end_line, embedding)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)"
                ),
                libsql::params![
                    chunk.chunk_id.as_str(),
                    chunk.file_path.as_str(),
                    chunk.symbol_name.as_deref(),
                    chunk.parent_scope.as_deref(),
                    chunk.language.as_str(),
                    chunk.content.as_str(),
                    i64::from(chunk.start_line),
                    i64::from(chunk.end_line),
                    vector_blob(&chunk.vector),
                ],
            )
            .await;
        if let Err(error) = result {
            let _ = conn.execute("ROLLBACK", ()).await;
            return Err(HiefError::Database(error));
        }
    }
    conn.execute("COMMIT", ())
        .await
        .map_err(HiefError::Database)?;

    Ok(chunks.len())
}

/// Search for code chunks semantically similar to the query.
///
/// The query text must first be embedded using the same model that
/// generated the stored embeddings. The caller provides the query vector.
pub async fn search(
    db: &Database,
    query_vector: &[f32],
    query: &SemanticQuery,
    config: &VectorConfig,
) -> Result<SemanticSearchOutcome> {
    if query_vector.is_empty() {
        return Ok(SemanticSearchOutcome {
            results: Vec::new(),
            cache_used: false,
            groundedness_score: Some(0.0),
        });
    }
    if query_vector.len() != config.dimensions {
        return Err(HiefError::Config(format!(
            "query embedding dimensions ({}) do not match configured dimensions ({})",
            query_vector.len(),
            config.dimensions
        )));
    }

    if let Some(cached) = read_semantic_cache(db, query_vector, query).await? {
        let contexts: Vec<&str> = cached.iter().map(|r| r.content.as_str()).collect();
        let groundedness = if contexts.is_empty() {
            0.0
        } else {
            crate::eval::scorer::groundedness_score(&query.query, &contexts)
        };
        let _ = crate::router::emit_shadow_signal(db, "semantic", Some(groundedness)).await;
        return Ok(SemanticSearchOutcome {
            results: cached,
            cache_used: true,
            groundedness_score: Some(groundedness),
        });
    }

    if !table_exists(db).await? {
        return Ok(SemanticSearchOutcome {
            results: Vec::new(),
            cache_used: false,
            groundedness_score: Some(0.0),
        });
    }

    // shortcut: exact scan instead of a DiskANN index, which can't take the
    // language filter. Fine to ~100k chunks; add libsql_vector_idx beyond that.
    let distance = distance_function(&config.metric)?;
    let top_k = i64::try_from(query.top_k).unwrap_or(i64::MAX);
    let mut rows = db
        .conn()
        .query(
            &format!(
                "SELECT chunk_id, file_path, symbol_name, parent_scope, language,
                        content, start_line, end_line,
                        {distance}(embedding, ?1) AS distance
                 FROM {TABLE_NAME}
                 WHERE (?2 IS NULL OR language = ?2)
                 ORDER BY distance ASC
                 LIMIT ?3"
            ),
            libsql::params![vector_blob(query_vector), query.language.as_deref(), top_k],
        )
        .await
        .map_err(HiefError::Database)?;

    let mut results = Vec::new();
    while let Some(row) = rows.next().await.map_err(HiefError::Database)? {
        let distance: f64 = row.get(8).map_err(HiefError::Database)?;
        results.push(SemanticResult {
            chunk_id: row.get(0).map_err(HiefError::Database)?,
            file_path: row.get(1).map_err(HiefError::Database)?,
            symbol_name: row.get(2).map_err(HiefError::Database)?,
            parent_scope: row.get(3).map_err(HiefError::Database)?,
            language: row.get(4).map_err(HiefError::Database)?,
            content: row.get(5).map_err(HiefError::Database)?,
            start_line: row.get::<i64>(6).map_err(HiefError::Database)? as u32,
            end_line: row.get::<i64>(7).map_err(HiefError::Database)? as u32,
            score: 1.0 / (1.0 + (distance as f32).max(0.0)),
        });
    }

    let contexts: Vec<&str> = results.iter().map(|r| r.content.as_str()).collect();
    let groundedness = if contexts.is_empty() {
        0.0
    } else {
        crate::eval::scorer::groundedness_score(&query.query, &contexts)
    };

    let _ = crate::router::emit_shadow_signal(db, "semantic", Some(groundedness)).await;

    write_semantic_cache(db, query_vector, query, &results).await?;
    Ok(SemanticSearchOutcome {
        results,
        cache_used: false,
        groundedness_score: Some(groundedness),
    })
}

/// Delete all embeddings for files that are no longer in the index.
pub async fn prune_deleted(
    db: &Database,
    deleted_file_paths: &[String],
    config: &VectorConfig,
) -> Result<usize> {
    if deleted_file_paths.is_empty() || !config.enabled || !table_exists(db).await? {
        return Ok(0);
    }

    let mut removed = 0usize;
    for path in deleted_file_paths {
        let n = db
            .conn()
            .execute(
                &format!("DELETE FROM {TABLE_NAME} WHERE file_path = ?1"),
                libsql::params![path.as_str()],
            )
            .await
            .map_err(HiefError::Database)?;
        removed += n as usize;
    }
    Ok(removed)
}

/// Get statistics about the vector store.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorStats {
    pub total_embeddings: usize,
    pub dimensions: usize,
    pub metric: String,
}

pub async fn stats(db: &Database, config: &VectorConfig) -> Result<VectorStats> {
    let total_embeddings = if config.enabled && table_exists(db).await? {
        let mut rows = db
            .conn()
            .query(&format!("SELECT COUNT(*) FROM {TABLE_NAME}"), ())
            .await
            .map_err(HiefError::Database)?;
        match rows.next().await.map_err(HiefError::Database)? {
            Some(row) => row.get::<i64>(0).map_err(HiefError::Database)? as usize,
            None => 0,
        }
    } else {
        0
    };

    Ok(VectorStats {
        total_embeddings,
        dimensions: config.dimensions,
        metric: config.metric.clone(),
    })
}

fn normalize_text(text: &str) -> String {
    text.to_ascii_lowercase()
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { ' ' })
        .collect()
}

fn tokenize(text: &str) -> Vec<String> {
    text.split_whitespace()
        .filter(|token| !token.is_empty())
        .map(std::string::ToString::to_string)
        .collect()
}

fn char_ngrams(text: &str, n: usize) -> Vec<String> {
    let compact: Vec<char> = text.chars().filter(|ch| !ch.is_whitespace()).collect();
    if compact.len() < n {
        return Vec::new();
    }

    compact
        .windows(n)
        .map(|window| window.iter().collect())
        .collect()
}

fn accumulate_feature(vector: &mut [f32], feature: &str, weight: f32) {
    let hash = blake3::hash(feature.as_bytes());
    let bytes = hash.as_bytes();
    let dims = vector.len();
    let primary = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize % dims;
    let secondary = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]) as usize % dims;
    let tertiary = u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]) as usize % dims;
    let sign_primary = if bytes[12] & 1 == 0 { 1.0 } else { -1.0 };
    let sign_secondary = if bytes[13] & 1 == 0 { 1.0 } else { -1.0 };

    vector[primary] += weight * sign_primary;
    vector[secondary] += (weight * 0.5) * sign_secondary;
    vector[tertiary] += weight * 0.25;
}

fn normalize_vector(vector: &mut [f32]) {
    let norm = vector.iter().map(|value| value * value).sum::<f32>().sqrt();
    if norm > 0.0 {
        for value in vector {
            *value /= norm;
        }
    }
}

fn chunk_id(chunk: &Chunk) -> String {
    format!(
        "{}:{}:{}:{}",
        chunk.file_path, chunk.start_line, chunk.end_line, chunk.content_hash
    )
}

fn validate_dimensions(chunks: &[EmbeddedChunk], dimensions: usize) -> Result<()> {
    for chunk in chunks {
        if chunk.vector.len() != dimensions {
            return Err(HiefError::Config(format!(
                "chunk '{}' has embedding dimensions {} but config expects {}",
                chunk.chunk_id,
                chunk.vector.len(),
                dimensions
            )));
        }
    }
    Ok(())
}

/// Little-endian f32 bytes, the on-disk form of an `F32_BLOB` column.
fn vector_blob(vector: &[f32]) -> Vec<u8> {
    vector
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect()
}

/// SQL function for the configured metric. Only these two names are ever
/// interpolated into SQL.
fn distance_function(metric: &str) -> Result<&'static str> {
    match metric {
        "cosine" => Ok("vector_distance_cos"),
        "l2" => Ok("vector_distance_l2"),
        other => Err(HiefError::Config(format!(
            "unsupported vector metric '{other}' (expected 'cosine' or 'l2')"
        ))),
    }
}

fn semantic_cache_key(query_vector: &[f32], query: &SemanticQuery) -> (String, String, String) {
    let query_fingerprint = blake3::hash(query.query.as_bytes()).to_hex().to_string();
    let mut embedding_bytes = Vec::with_capacity(query_vector.len() * 4);
    for value in query_vector {
        embedding_bytes.extend_from_slice(&value.to_ne_bytes());
    }
    let embedding_hash = blake3::hash(&embedding_bytes).to_hex().to_string();
    let language_scope = query.language.clone().unwrap_or_else(|| "*".to_string());
    (query_fingerprint, embedding_hash, language_scope)
}

async fn read_semantic_cache(
    db: &Database,
    query_vector: &[f32],
    query: &SemanticQuery,
) -> Result<Option<Vec<SemanticResult>>> {
    let (query_fingerprint, embedding_hash, language_scope) =
        semantic_cache_key(query_vector, query);
    let now = unix_now();

    let mut rows = db
        .conn()
        .query(
            "SELECT result_json, result_payload_hash FROM semantic_cache
             WHERE query_fingerprint = ?1
               AND embedding_hash = ?2
               AND language_scope = ?3
               AND expires_at > ?4
             ORDER BY created_at DESC
             LIMIT 1",
            libsql::params![
                query_fingerprint.as_str(),
                embedding_hash.as_str(),
                language_scope.as_str(),
                now,
            ],
        )
        .await
        .map_err(crate::errors::HiefError::Database)?;

    if let Some(row) = rows
        .next()
        .await
        .map_err(crate::errors::HiefError::Database)?
    {
        let json: String = row.get(0).map_err(crate::errors::HiefError::Database)?;
        let payload_hash: String = row.get(1).map_err(crate::errors::HiefError::Database)?;
        let computed_hash = blake3::hash(json.as_bytes()).to_hex().to_string();
        if computed_hash != payload_hash {
            return Ok(None);
        }
        let results: Vec<SemanticResult> = serde_json::from_str(&json).map_err(|e| {
            HiefError::Other(format!("failed to parse semantic cache payload: {e}"))
        })?;
        return Ok(Some(results));
    }

    Ok(None)
}

async fn write_semantic_cache(
    db: &Database,
    query_vector: &[f32],
    query: &SemanticQuery,
    results: &[SemanticResult],
) -> Result<()> {
    if results.is_empty() {
        return Ok(());
    }

    let (query_fingerprint, embedding_hash, language_scope) =
        semantic_cache_key(query_vector, query);
    let result_json = serde_json::to_string(results).map_err(|e| {
        HiefError::Other(format!("failed to serialize semantic cache payload: {e}"))
    })?;
    let result_payload_hash = blake3::hash(result_json.as_bytes()).to_hex().to_string();
    let expires_at = unix_now() + SEMANTIC_CACHE_TTL_SECS;

    db.conn()
        .execute(
            "INSERT INTO semantic_cache
             (query_fingerprint, embedding_hash, language_scope, result_payload_hash, result_json, expires_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(query_fingerprint, embedding_hash, language_scope)
             DO UPDATE SET
                 result_payload_hash = excluded.result_payload_hash,
                 result_json = excluded.result_json,
                 expires_at = excluded.expires_at",
            libsql::params![
                query_fingerprint,
                embedding_hash,
                language_scope,
                result_payload_hash,
                result_json,
                expires_at,
            ],
        )
        .await
        .map_err(crate::errors::HiefError::Database)?;

    Ok(())
}

fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;

    #[test]
    fn test_default_config() {
        let config = VectorConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.dimensions, 384);
        assert_eq!(config.metric, "cosine");
    }

    #[test]
    fn test_semantic_query() {
        let query = SemanticQuery::new("authentication logic");
        assert_eq!(query.query, "authentication logic");
        assert_eq!(query.top_k, 10);
        assert!(query.language.is_none());
    }

    #[test]
    fn test_embed_text_is_deterministic() {
        let left = embed_text("authentication logic", 16).unwrap();
        let right = embed_text("authentication logic", 16).unwrap();
        assert_eq!(left, right);
    }

    #[tokio::test]
    async fn test_init_disabled() {
        let db = Database::open_memory().await.unwrap();
        assert!(!init(&db, &VectorConfig::default()).await.unwrap());
        assert!(!table_exists(&db).await.unwrap());
    }

    #[tokio::test]
    async fn test_init_enabled_reports_creation_once() {
        let db = Database::open_memory().await.unwrap();
        let config = VectorConfig {
            enabled: true,
            ..Default::default()
        };
        assert!(init(&db, &config).await.unwrap());
        assert!(!init(&db, &config).await.unwrap());
        assert!(table_exists(&db).await.unwrap());
    }

    #[tokio::test]
    async fn test_unknown_metric_is_rejected() {
        let db = Database::open_memory().await.unwrap();
        let config = VectorConfig {
            enabled: true,
            metric: "dot".to_string(),
            ..Default::default()
        };
        assert!(init(&db, &config).await.is_err());
    }

    #[tokio::test]
    async fn test_search_empty() {
        let config = VectorConfig::default();
        let db = Database::open_memory().await.unwrap();
        let query = SemanticQuery::new("test");
        let outcome = search(&db, &[], &query, &config).await.unwrap();
        assert!(outcome.results.is_empty());
        assert!(!outcome.cache_used);
    }

    #[tokio::test]
    async fn test_store_and_search_roundtrip() {
        let config = VectorConfig {
            enabled: true,
            dimensions: 32,
            ..Default::default()
        };
        let db = Database::open_memory().await.unwrap();
        init(&db, &config).await.unwrap();

        let chunks = vec![EmbeddedChunk {
            chunk_id: "src/auth.rs:1:3:hash".to_string(),
            file_path: "src/auth.rs".to_string(),
            symbol_name: Some("authenticate_user".to_string()),
            parent_scope: None,
            language: "rust".to_string(),
            content: "fn authenticate_user(token: &str) -> bool { token.starts_with(\"bearer\") }"
                .to_string(),
            start_line: 1,
            end_line: 3,
            vector: embed_text("authenticate user bearer token", config.dimensions).unwrap(),
        }];

        store_embeddings(&db, &chunks, &config).await.unwrap();

        let mut query = SemanticQuery::new("bearer token auth");
        query.top_k = 5;
        let outcome = search(
            &db,
            &embed_text(&query.query, config.dimensions).unwrap(),
            &query,
            &config,
        )
        .await
        .unwrap();

        assert_eq!(outcome.results.len(), 1);
        assert_eq!(outcome.results[0].file_path, "src/auth.rs");
        assert_eq!(
            outcome.results[0].symbol_name.as_deref(),
            Some("authenticate_user")
        );
        assert!(outcome.results[0].score > 0.0);
        assert!(!outcome.cache_used);
    }

    #[tokio::test]
    async fn test_prune_deleted_removes_file_embeddings() {
        let config = VectorConfig {
            enabled: true,
            dimensions: 24,
            ..Default::default()
        };
        let db = Database::open_memory().await.unwrap();
        init(&db, &config).await.unwrap();

        let chunks = vec![EmbeddedChunk {
            chunk_id: "src/old.rs:1:1:hash".to_string(),
            file_path: "src/old.rs".to_string(),
            symbol_name: Some("old_fn".to_string()),
            parent_scope: None,
            language: "rust".to_string(),
            content: "fn old_fn() {}".to_string(),
            start_line: 1,
            end_line: 1,
            vector: embed_text("legacy cleanup function", config.dimensions).unwrap(),
        }];

        store_embeddings(&db, &chunks, &config).await.unwrap();
        let removed = prune_deleted(&db, &["src/old.rs".to_string()], &config)
            .await
            .unwrap();
        assert_eq!(removed, 1);

        let stats = stats(&db, &config).await.unwrap();
        assert_eq!(stats.total_embeddings, 0);
    }

    #[tokio::test]
    async fn test_search_language_filter() {
        let config = VectorConfig {
            enabled: true,
            dimensions: 24,
            ..Default::default()
        };
        let db = Database::open_memory().await.unwrap();
        init(&db, &config).await.unwrap();

        let chunks = vec![
            EmbeddedChunk {
                chunk_id: "src/auth.rs:1:1:a".to_string(),
                file_path: "src/auth.rs".to_string(),
                symbol_name: Some("auth_rust".to_string()),
                parent_scope: None,
                language: "rust".to_string(),
                content: "fn auth_rust() -> bool { true }".to_string(),
                start_line: 1,
                end_line: 1,
                vector: embed_text("authentication logic", config.dimensions).unwrap(),
            },
            EmbeddedChunk {
                chunk_id: "src/auth.py:1:1:b".to_string(),
                file_path: "src/auth.py".to_string(),
                symbol_name: Some("auth_python".to_string()),
                parent_scope: None,
                language: "python".to_string(),
                content: "def auth_python():\n    return True".to_string(),
                start_line: 1,
                end_line: 2,
                vector: embed_text("authentication logic", config.dimensions).unwrap(),
            },
        ];
        store_embeddings(&db, &chunks, &config).await.unwrap();

        let mut query = SemanticQuery::new("authentication logic");
        query.language = Some("python".to_string());
        let outcome = search(
            &db,
            &embed_text(&query.query, config.dimensions).unwrap(),
            &query,
            &config,
        )
        .await
        .unwrap();

        assert_eq!(outcome.results.len(), 1);
        assert_eq!(outcome.results[0].language, "python");
        assert!(!outcome.cache_used);
    }
}
