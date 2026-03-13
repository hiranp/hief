//! Semantic vector search via LanceDB.
//!
//! This module provides embedded vector storage and similarity search for code
//! chunks. It complements the FTS5 keyword search in [`search`] and the
//! structural pattern search in [`structural`].
//!
//! HIEF does not bundle an LLM. For this milestone, embeddings are generated
//! locally with a deterministic hashing-based encoder so semantic search is
//! self-contained and testable without any external model service.

#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::sync::Arc;

use arrow_array::types::Float32Type;
use arrow_array::{Array, Float32Array, Int64Array, RecordBatch, RecordBatchIterator, StringArray};
use arrow_schema::{DataType, Field, Schema};
use futures::TryStreamExt;
use lancedb::query::{ExecutableQuery, QueryBase};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tracing::info;

use crate::errors::{HiefError, Result};

use super::chunker::Chunk;

const TABLE_NAME: &str = "code_chunks";
const VECTOR_COLUMN: &str = "vector";

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

/// Convert indexed chunks into deterministic embeddings for LanceDB storage.
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
        accumulate_feature(&mut vector, token, 1.0 + (token.len().min(12) as f32 / 12.0));
    }

    for trigram in char_ngrams(&normalized, 3) {
        accumulate_feature(&mut vector, &trigram, 0.35);
    }

    normalize_vector(&mut vector);
    Ok(vector)
}

/// Returns the path to the LanceDB vectors directory.
pub fn vectors_dir(project_root: &Path) -> PathBuf {
    project_root.join(".hief").join("vectors")
}

/// Initialize the vector store. Creates the LanceDB directory and table
/// if they don't exist.
pub async fn init(project_root: &Path, config: &VectorConfig) -> Result<()> {
    if !config.enabled {
        info!("Vector search is disabled in configuration");
        return Ok(());
    }

    let dir = vectors_dir(project_root);
    if !dir.exists() {
        std::fs::create_dir_all(&dir)?;
        info!("Created vector store directory: {}", dir.display());
    }

    let db = connect_db(&dir).await?;
    let table_names = db
        .table_names()
        .execute()
        .await
        .map_err(map_vector_error)?;
    if !table_names.iter().any(|name| name == TABLE_NAME) {
        db.create_empty_table(TABLE_NAME, vector_schema(config.dimensions))
            .execute()
            .await
            .map_err(map_vector_error)?;
    }

    info!(
        "Vector store initialized (dimensions={}, metric={})",
        config.dimensions, config.metric
    );
    Ok(())
}

/// Store embeddings for a batch of code chunks.
///
/// This is called after the index build phase, once embeddings have been
/// generated for new/updated chunks.
pub async fn store_embeddings(
    project_root: &Path,
    chunks: &[EmbeddedChunk],
    config: &VectorConfig,
) -> Result<usize> {
    if chunks.is_empty() {
        return Ok(0);
    }

    validate_dimensions(chunks, config.dimensions)?;
    if !vectors_dir(project_root).exists() {
        init(project_root, config).await?;
    }

    let table = open_table(project_root).await?;
    let chunk_ids: Vec<String> = chunks.iter().map(|chunk| chunk.chunk_id.clone()).collect();
    if !chunk_ids.is_empty() {
        table
            .delete(&in_predicate("chunk_id", &chunk_ids))
            .await
            .map_err(map_vector_error)?;
    }
    let batch = chunks_to_record_batch(chunks, config.dimensions)?;
    let schema = batch.schema();
    let reader: Box<dyn arrow_array::RecordBatchReader + Send> = Box::new(
        RecordBatchIterator::new(vec![Ok(batch)], schema),
    );
    table
        .add(reader)
        .execute()
        .await
        .map_err(map_vector_error)?;

    Ok(chunks.len())
}

/// Search for code chunks semantically similar to the query.
///
/// The query text must first be embedded using the same model that
/// generated the stored embeddings. The caller provides the query vector.
pub async fn search(
    project_root: &Path,
    query_vector: &[f32],
    query: &SemanticQuery,
    config: &VectorConfig,
) -> Result<Vec<SemanticResult>> {
    if query_vector.is_empty() {
        return Ok(Vec::new());
    }
    if query_vector.len() != config.dimensions {
        return Err(HiefError::Config(format!(
            "query embedding dimensions ({}) do not match configured dimensions ({})",
            query_vector.len(),
            config.dimensions
        )));
    }

    let dir = vectors_dir(project_root);
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let table = match open_table(project_root).await {
        Ok(table) => table,
        Err(_) => return Ok(Vec::new()),
    };

    let mut vector_query = table
        .query()
        .limit(query.top_k)
        .nearest_to(query_vector)
        .map_err(map_vector_error)?;

    vector_query = vector_query.distance_type(distance_type(&config.metric)?);
    if let Some(language) = &query.language {
        vector_query = vector_query.only_if(&eq_predicate("language", language));
    }

    let batches = vector_query
        .execute()
        .await
        .map_err(map_vector_error)?
        .try_collect::<Vec<_>>()
        .await
        .map_err(map_vector_error)?;

    let mut results = Vec::new();
    for batch in batches {
        results.extend(batch_to_results(&batch)?);
    }
    results.truncate(query.top_k);
    Ok(results)
}

/// Delete all embeddings for files that are no longer in the index.
pub async fn prune_deleted(
    project_root: &Path,
    deleted_file_paths: &[String],
    config: &VectorConfig,
) -> Result<usize> {
    if deleted_file_paths.is_empty() {
        return Ok(0);
    }

    if !config.enabled {
        return Ok(0);
    }

    let dir = vectors_dir(project_root);
    if !dir.exists() {
        return Ok(0);
    }

    let table = match open_table(project_root).await {
        Ok(table) => table,
        Err(_) => return Ok(0),
    };
    let predicate = in_predicate("file_path", deleted_file_paths);
    let rows_to_delete = table
        .count_rows(Some(predicate.clone()))
        .await
        .map_err(map_vector_error)?;
    table.delete(&predicate).await.map_err(map_vector_error)?;
    Ok(rows_to_delete)
}

/// Get statistics about the vector store.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorStats {
    pub total_embeddings: usize,
    pub dimensions: usize,
    pub metric: String,
    pub store_size_bytes: u64,
}

pub async fn stats(project_root: &Path, config: &VectorConfig) -> Result<VectorStats> {
    let dir = vectors_dir(project_root);
    let store_size = if dir.exists() { dir_size(&dir) } else { 0 };
    let total_embeddings = if config.enabled && dir.exists() {
        match open_table(project_root).await {
            Ok(table) => table.count_rows(None).await.map_err(map_vector_error)?,
            Err(_) => 0,
        }
    } else {
        0
    };

    Ok(VectorStats {
        total_embeddings,
        dimensions: config.dimensions,
        metric: config.metric.clone(),
        store_size_bytes: store_size,
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

async fn connect_db(path: &Path) -> Result<lancedb::connection::Connection> {
    lancedb::connect(path.to_string_lossy().as_ref())
        .execute()
        .await
        .map_err(map_vector_error)
}

async fn open_table(project_root: &Path) -> Result<lancedb::Table> {
    let dir = vectors_dir(project_root);
    let db = connect_db(&dir).await?;
    db.open_table(TABLE_NAME)
        .execute()
        .await
        .map_err(map_vector_error)
}

fn vector_schema(dimensions: usize) -> Arc<Schema> {
    Arc::new(Schema::new(vec![
        Field::new("chunk_id", DataType::Utf8, false),
        Field::new("file_path", DataType::Utf8, false),
        Field::new("symbol_name", DataType::Utf8, true),
        Field::new("parent_scope", DataType::Utf8, true),
        Field::new("language", DataType::Utf8, false),
        Field::new("content", DataType::Utf8, false),
        Field::new("start_line", DataType::Int64, false),
        Field::new("end_line", DataType::Int64, false),
        Field::new(
            VECTOR_COLUMN,
            DataType::FixedSizeList(
                Arc::new(Field::new("item", DataType::Float32, true)),
                dimensions as i32,
            ),
            false,
        ),
    ]))
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

fn chunks_to_record_batch(chunks: &[EmbeddedChunk], dimensions: usize) -> Result<RecordBatch> {
    let schema = vector_schema(dimensions);
    let chunk_ids = StringArray::from(
        chunks
            .iter()
            .map(|chunk| chunk.chunk_id.as_str())
            .collect::<Vec<_>>(),
    );
    let file_paths = StringArray::from(
        chunks
            .iter()
            .map(|chunk| chunk.file_path.as_str())
            .collect::<Vec<_>>(),
    );
    let symbol_names = StringArray::from(
        chunks
            .iter()
            .map(|chunk| chunk.symbol_name.as_deref())
            .collect::<Vec<_>>(),
    );
    let parent_scopes = StringArray::from(
        chunks
            .iter()
            .map(|chunk| chunk.parent_scope.as_deref())
            .collect::<Vec<_>>(),
    );
    let languages = StringArray::from(
        chunks
            .iter()
            .map(|chunk| chunk.language.as_str())
            .collect::<Vec<_>>(),
    );
    let contents = StringArray::from(
        chunks
            .iter()
            .map(|chunk| chunk.content.as_str())
            .collect::<Vec<_>>(),
    );
    let start_lines = Int64Array::from(
        chunks
            .iter()
            .map(|chunk| chunk.start_line as i64)
            .collect::<Vec<_>>(),
    );
    let end_lines = Int64Array::from(
        chunks
            .iter()
            .map(|chunk| chunk.end_line as i64)
            .collect::<Vec<_>>(),
    );
    let vectors = arrow_array::FixedSizeListArray::from_iter_primitive::<Float32Type, _, _>(
        chunks
            .iter()
            .map(|chunk| Some(chunk.vector.iter().copied().map(Some))),
        dimensions as i32,
    );

    RecordBatch::try_new(
        schema,
        vec![
            Arc::new(chunk_ids),
            Arc::new(file_paths),
            Arc::new(symbol_names),
            Arc::new(parent_scopes),
            Arc::new(languages),
            Arc::new(contents),
            Arc::new(start_lines),
            Arc::new(end_lines),
            Arc::new(vectors),
        ],
    )
    .map_err(|error| HiefError::Other(format!("failed to build vector batch: {error}")))
}

fn batch_to_results(batch: &RecordBatch) -> Result<Vec<SemanticResult>> {
    let chunk_ids = batch
        .column_by_name("chunk_id")
        .and_then(|column| column.as_any().downcast_ref::<StringArray>())
        .ok_or_else(|| HiefError::Other("missing chunk_id column in vector search result".to_string()))?;
    let file_paths = batch
        .column_by_name("file_path")
        .and_then(|column| column.as_any().downcast_ref::<StringArray>())
        .ok_or_else(|| HiefError::Other("missing file_path column in vector search result".to_string()))?;
    let symbol_names = batch
        .column_by_name("symbol_name")
        .and_then(|column| column.as_any().downcast_ref::<StringArray>())
        .ok_or_else(|| HiefError::Other("missing symbol_name column in vector search result".to_string()))?;
    let parent_scopes = batch
        .column_by_name("parent_scope")
        .and_then(|column| column.as_any().downcast_ref::<StringArray>())
        .ok_or_else(|| HiefError::Other("missing parent_scope column in vector search result".to_string()))?;
    let languages = batch
        .column_by_name("language")
        .and_then(|column| column.as_any().downcast_ref::<StringArray>())
        .ok_or_else(|| HiefError::Other("missing language column in vector search result".to_string()))?;
    let contents = batch
        .column_by_name("content")
        .and_then(|column| column.as_any().downcast_ref::<StringArray>())
        .ok_or_else(|| HiefError::Other("missing content column in vector search result".to_string()))?;
    let start_lines = batch
        .column_by_name("start_line")
        .and_then(|column| column.as_any().downcast_ref::<Int64Array>())
        .ok_or_else(|| HiefError::Other("missing start_line column in vector search result".to_string()))?;
    let end_lines = batch
        .column_by_name("end_line")
        .and_then(|column| column.as_any().downcast_ref::<Int64Array>())
        .ok_or_else(|| HiefError::Other("missing end_line column in vector search result".to_string()))?;

    let mut results = Vec::with_capacity(batch.num_rows());
    for row in 0..batch.num_rows() {
        let distance = distance_at(batch, row)?;
        results.push(SemanticResult {
            chunk_id: chunk_ids.value(row).to_string(),
            file_path: file_paths.value(row).to_string(),
            symbol_name: (!symbol_names.is_null(row)).then(|| symbol_names.value(row).to_string()),
            parent_scope: (!parent_scopes.is_null(row)).then(|| parent_scopes.value(row).to_string()),
            language: languages.value(row).to_string(),
            content: contents.value(row).to_string(),
            start_line: start_lines.value(row) as u32,
            end_line: end_lines.value(row) as u32,
            score: 1.0 / (1.0 + distance.max(0.0)),
        });
    }

    Ok(results)
}

fn distance_at(batch: &RecordBatch, row: usize) -> Result<f32> {
    if let Some(column) = batch.column_by_name("_distance") {
        if let Some(distances) = column.as_any().downcast_ref::<Float32Array>() {
            return Ok(distances.value(row));
        }
    }
    Ok(0.0)
}

fn eq_predicate(column: &str, value: &str) -> String {
    format!("{column} = '{}'", sql_quote(value))
}

fn in_predicate(column: &str, values: &[String]) -> String {
    let quoted = values
        .iter()
        .map(|value| format!("'{}'", sql_quote(value)))
        .collect::<Vec<_>>()
        .join(", ");
    format!("{column} IN ({quoted})")
}

fn sql_quote(value: &str) -> String {
    value.replace('\'', "''")
}

fn distance_type(metric: &str) -> Result<lancedb::DistanceType> {
    match metric {
        "cosine" => Ok(lancedb::DistanceType::Cosine),
        "l2" => Ok(lancedb::DistanceType::L2),
        other => Err(HiefError::Config(format!(
            "unsupported vector metric '{other}' (expected 'cosine' or 'l2')"
        ))),
    }
}

fn map_vector_error<E: std::fmt::Display>(error: E) -> HiefError {
    HiefError::Other(format!("vector store error: {error}"))
}

/// Calculate total size of a directory.
fn dir_size(path: &Path) -> u64 {
    let mut total = 0u64;
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            let metadata = entry.metadata();
            if let Ok(meta) = metadata {
                if meta.is_file() {
                    total += meta.len();
                } else if meta.is_dir() {
                    total += dir_size(&entry.path());
                }
            }
        }
    }
    total
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn test_vectors_dir() {
        let dir = vectors_dir(Path::new("/tmp/project"));
        assert_eq!(dir, PathBuf::from("/tmp/project/.hief/vectors"));
    }

    #[tokio::test]
    async fn test_init_disabled() {
        let config = VectorConfig::default(); // enabled = false
        let tmp = tempfile::tempdir().unwrap();
        let result = init(tmp.path(), &config).await;
        assert!(result.is_ok());
        // Should not create directory when disabled
        assert!(!vectors_dir(tmp.path()).exists());
    }

    #[tokio::test]
    async fn test_init_enabled() {
        let config = VectorConfig {
            enabled: true,
            ..Default::default()
        };
        let tmp = tempfile::tempdir().unwrap();
        let result = init(tmp.path(), &config).await;
        assert!(result.is_ok());
        assert!(vectors_dir(tmp.path()).exists());
    }

    #[tokio::test]
    async fn test_search_empty() {
        let config = VectorConfig::default();
        let tmp = tempfile::tempdir().unwrap();
        let query = SemanticQuery::new("test");
        let results = search(tmp.path(), &[], &query, &config).await.unwrap();
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_store_and_search_roundtrip() {
        let config = VectorConfig {
            enabled: true,
            dimensions: 32,
            ..Default::default()
        };
        let tmp = tempfile::tempdir().unwrap();
        init(tmp.path(), &config).await.unwrap();

        let chunks = vec![EmbeddedChunk {
            chunk_id: "src/auth.rs:1:3:hash".to_string(),
            file_path: "src/auth.rs".to_string(),
            symbol_name: Some("authenticate_user".to_string()),
            parent_scope: None,
            language: "rust".to_string(),
            content: "fn authenticate_user(token: &str) -> bool { token.starts_with(\"bearer\") }".to_string(),
            start_line: 1,
            end_line: 3,
            vector: embed_text("authenticate user bearer token", config.dimensions).unwrap(),
        }];

        store_embeddings(tmp.path(), &chunks, &config).await.unwrap();

        let mut query = SemanticQuery::new("bearer token auth");
        query.top_k = 5;
        let results = search(
            tmp.path(),
            &embed_text(&query.query, config.dimensions).unwrap(),
            &query,
            &config,
        )
        .await
        .unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].file_path, "src/auth.rs");
        assert_eq!(results[0].symbol_name.as_deref(), Some("authenticate_user"));
        assert!(results[0].score > 0.0);
    }

    #[tokio::test]
    async fn test_prune_deleted_removes_file_embeddings() {
        let config = VectorConfig {
            enabled: true,
            dimensions: 24,
            ..Default::default()
        };
        let tmp = tempfile::tempdir().unwrap();
        init(tmp.path(), &config).await.unwrap();

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

        store_embeddings(tmp.path(), &chunks, &config).await.unwrap();
        let removed = prune_deleted(tmp.path(), &["src/old.rs".to_string()], &config)
            .await
            .unwrap();
        assert_eq!(removed, 1);

        let stats = stats(tmp.path(), &config).await.unwrap();
        assert_eq!(stats.total_embeddings, 0);
    }

    #[tokio::test]
    async fn test_search_language_filter() {
        let config = VectorConfig {
            enabled: true,
            dimensions: 24,
            ..Default::default()
        };
        let tmp = tempfile::tempdir().unwrap();
        init(tmp.path(), &config).await.unwrap();

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
        store_embeddings(tmp.path(), &chunks, &config).await.unwrap();

        let mut query = SemanticQuery::new("authentication logic");
        query.language = Some("python".to_string());
        let results = search(
            tmp.path(),
            &embed_text(&query.query, config.dimensions).unwrap(),
            &query,
            &config,
        )
        .await
        .unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].language, "python");
    }
}
