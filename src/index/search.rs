//! FTS5 full-text search over indexed code chunks.

use schemars::JsonSchema;
use serde::Serialize;
use tracing::debug;

use crate::db::Database;
use crate::errors::{HiefError, Result};

/// Parameters for a code search query.
#[derive(Debug, Clone)]
pub struct SearchQuery {
    pub text: String,
    pub top_k: usize,
    pub language: Option<String>,
    pub symbol_kind: Option<String>,
    pub file_pattern: Option<String>,
}

impl SearchQuery {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            top_k: 10,
            language: None,
            symbol_kind: None,
            file_pattern: None,
        }
    }

    #[allow(dead_code)]
    pub fn with_top_k(mut self, k: usize) -> Self {
        self.top_k = k;
        self
    }

    #[allow(dead_code)]
    pub fn with_language(mut self, lang: impl Into<String>) -> Self {
        self.language = Some(lang.into());
        self
    }

    #[allow(dead_code)]
    pub fn with_symbol_kind(mut self, kind: impl Into<String>) -> Self {
        self.symbol_kind = Some(kind.into());
        self
    }
}

/// A search result with ranking metadata.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct SearchResult {
    pub file_path: String,
    pub symbol_name: Option<String>,
    pub symbol_kind: Option<String>,
    pub parent_scope: Option<String>,
    pub language: String,
    pub content: String,
    pub start_line: u32,
    pub end_line: u32,
    pub rank: f64,
    pub snippet: String,
    pub groundedness_score: Option<f64>,
}

/// Retrieval metadata attached to semantic search responses.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct RetrievalMetadata {
    pub strategy: String,
    pub cache_used: bool,
    pub quality_signal: Option<f64>,
}

/// Construct placeholder retrieval metadata for a semantic response.
pub fn semantic_retrieval_metadata(
    strategy: impl Into<String>,
    cache_used: bool,
    quality_signal: Option<f64>,
) -> RetrievalMetadata {
    RetrievalMetadata {
        strategy: strategy.into(),
        cache_used,
        quality_signal,
    }
}

/// Execute a full-text search query against the chunks index.
pub async fn search(db: &Database, query: &SearchQuery) -> Result<Vec<SearchResult>> {
    let conn = db.conn();

    // Build the SQL query dynamically based on filters
    let mut sql = String::from(
        "SELECT c.file_path, c.symbol_name, c.symbol_kind, c.parent_scope,
                c.language, c.content, c.start_line, c.end_line,
                rank,
                snippet(chunks_fts, 2, '>>>', '<<<', '...', 32) as snip
         FROM chunks_fts
         JOIN chunks c ON c.id = chunks_fts.rowid
         WHERE chunks_fts MATCH ?1",
    );

    let mut param_idx = 2;
    let mut params: Vec<String> = vec![query.text.clone()];

    if let Some(lang) = &query.language {
        sql.push_str(&format!(" AND c.language = ?{}", param_idx));
        params.push(lang.clone());
        param_idx += 1;
    }

    if let Some(kind) = &query.symbol_kind {
        sql.push_str(&format!(" AND c.symbol_kind = ?{}", param_idx));
        params.push(kind.clone());
        param_idx += 1;
    }

    if let Some(pattern) = &query.file_pattern {
        sql.push_str(&format!(" AND c.file_path GLOB ?{}", param_idx));
        params.push(pattern.clone());
        param_idx += 1;
    }

    let _ = param_idx; // suppress unused warning

    sql.push_str(&format!(" ORDER BY rank LIMIT {}", query.top_k));

    debug!("Search SQL: {} | params: {:?}", sql, params);

    // Execute with dynamic params via Vec<libsql::Value> to handle any
    // number of filter parameters without a brittle match-on-count branch.
    let libsql_params: Vec<libsql::Value> = params
        .iter()
        .map(|s| libsql::Value::from(s.as_str()))
        .collect();
    let mut rows = conn
        .query(&sql, libsql_params)
        .await
        .map_err(HiefError::Database)?;

    let mut results = Vec::new();

    while let Some(row) = rows.next().await.map_err(HiefError::Database)? {
        let file_path: String = row.get(0).map_err(HiefError::Database)?;
        let symbol_name: Option<String> = row.get(1).ok();
        let symbol_kind: Option<String> = row.get(2).ok();
        let parent_scope: Option<String> = row.get(3).ok();
        let language: String = row.get(4).map_err(HiefError::Database)?;
        let content: String = row.get(5).map_err(HiefError::Database)?;
        let start_line: i64 = row.get(6).map_err(HiefError::Database)?;
        let end_line: i64 = row.get(7).map_err(HiefError::Database)?;
        let rank: f64 = row.get(8).map_err(HiefError::Database)?;
        let snippet: String = row.get::<String>(9).unwrap_or_default();

        results.push(SearchResult {
            file_path,
            symbol_name,
            symbol_kind,
            parent_scope,
            language,
            content,
            start_line: start_line as u32,
            end_line: end_line as u32,
            rank,
            snippet,
            groundedness_score: None,
        });
    }

    // Compute groundedness per-result so downstream callers (PAVL, ranking
    // boosts) can distinguish high-overlap from low-overlap results rather
    // than receiving an identical aggregate score stamped on every item.
    for result in &mut results {
        result.groundedness_score = Some(crate::eval::scorer::groundedness_score(
            &query.text,
            &[result.content.as_str()],
        ));
    }

    debug!(
        "Search returned {} results for '{}'",
        results.len(),
        query.text
    );
    Ok(results)
}

/// Git blame for a specific file range (on-demand, shells out to git).
pub async fn git_blame_range(file: &str, start_line: u32, end_line: u32) -> Result<String> {
    // Basic validation against flag injection and path traversal
    if file.starts_with('-') || file.contains("..") {
        return Err(HiefError::SecurityViolation(format!(
            "Invalid file path for git blame: {}",
            file
        )));
    }

    let output = tokio::process::Command::new("git")
        .args([
            "blame",
            "-L",
            &format!("{},{}", start_line + 1, end_line + 1), // git blame is 1-indexed
            "--porcelain",
            "--", // Explicitly terminate flags
            file,
        ])
        .output()
        .await?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(HiefError::Other(format!("git blame failed: {}", stderr)));
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_git_blame_range_security() {
        // Test flag injection
        let res = git_blame_range("--version", 0, 0).await;
        assert!(res.is_err());
        assert!(res.unwrap_err().to_string().contains("security violation"));

        // Test path traversal
        let res = git_blame_range("../../../etc/passwd", 0, 0).await;
        assert!(res.is_err());
        assert!(res.unwrap_err().to_string().contains("security violation"));
    }
}
