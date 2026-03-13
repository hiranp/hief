//! `hief index` — code indexing and search commands.

use std::path::Path;

use crate::config::Config;
use crate::db::Database;
use crate::errors::{HiefError, Result};

/// Build or update the code index.
pub async fn index_build(
    db: &Database,
    project_root: &Path,
    config: &Config,
    json: bool,
) -> Result<()> {
    let report = crate::index::build(db, project_root, &config.index, &config.vectors).await?;

    if json {
        println!("{}", serde_json::to_string_pretty(&report).unwrap());
    } else {
        println!(
            "📦 Indexed {} files ({} new, {} updated, {} removed)",
            report.files_new + report.files_updated,
            report.files_new,
            report.files_updated,
            report.files_removed,
        );
        println!(
            "   {} total chunks, {}ms",
            report.total_chunks, report.duration_ms
        );
    }

    Ok(())
}

/// Search the code index.
pub async fn index_search(
    db: &Database,
    query: &str,
    top_k: usize,
    language: Option<&str>,
    kind: Option<&str>,
    json: bool,
) -> Result<()> {
    let mut search_query = crate::index::search::SearchQuery::new(query);
    search_query.top_k = top_k;
    if let Some(lang) = language {
        search_query.language = Some(lang.to_string());
    }
    if let Some(k) = kind {
        search_query.symbol_kind = Some(k.to_string());
    }

    let results = crate::index::search(db, &search_query).await?;

    if json {
        println!("{}", serde_json::to_string_pretty(&results).unwrap());
    } else if results.is_empty() {
        println!("No results found for '{}'", query);
    } else {
        println!("Found {} results for '{}':\n", results.len(), query);
        for (i, r) in results.iter().enumerate() {
            let symbol = r.symbol_name.as_deref().unwrap_or("(anonymous)");
            let kind = r.symbol_kind.as_deref().unwrap_or("");
            println!(
                "  {}. {} [{}] — {}:{}–{}",
                i + 1,
                symbol,
                kind,
                r.file_path,
                r.start_line,
                r.end_line,
            );
            // Show snippet (first 3 lines)
            for line in r.content.lines().take(3) {
                println!("     {}", line);
            }
            println!();
        }
    }

    Ok(())
}

/// Structural search using ast-grep patterns.
pub fn index_structural(
    project_root: &Path,
    pattern: &str,
    language: &str,
    top_k: usize,
    json: bool,
) -> Result<()> {
    let query = crate::index::structural::StructuralQuery {
        pattern: pattern.to_string(),
        language: language.to_string(),
        top_k,
    };

    let matches = crate::index::structural::search(project_root, &query)?;

    if json {
        println!("{}", serde_json::to_string_pretty(&matches).unwrap());
    } else if matches.is_empty() {
        println!(
            "No structural matches found for pattern '{}' in {} files",
            pattern, language
        );
    } else {
        println!(
            "Found {} structural matches for '{}':\n",
            matches.len(),
            pattern
        );
        for (i, m) in matches.iter().enumerate() {
            println!(
                "  {}. {}:{}–{} (col {}–{})",
                i + 1,
                m.file_path,
                m.start_line,
                m.end_line,
                m.start_col,
                m.end_col,
            );
            println!(
                "     Match: {}",
                m.matched_text.lines().next().unwrap_or("")
            );
            // Show context (first 3 lines)
            for line in m.context.lines().take(3) {
                println!("     {}", line);
            }
            println!();
        }
    }

    Ok(())
}

/// Show index statistics.
pub async fn index_status(db: &Database, project_root: &Path, json: bool) -> Result<()> {
    let stats = crate::index::status(db, project_root).await?;

    if json {
        println!("{}", serde_json::to_string_pretty(&stats).unwrap());
    } else {
        println!("📊 Index Status:");
        println!("   Files: {}", stats.total_files);
        println!("   Chunks: {}", stats.total_chunks);
        println!("   DB size: {} bytes", stats.db_size_bytes);
        if let Some(ts) = stats.last_indexed {
            println!("   Last indexed: {}", ts);
        }
        println!("   Languages:");
        for (lang, count) in &stats.languages {
            println!("     {}: {} files", lang, count);
        }
    }

    Ok(())
}

/// Semantic search using vector embeddings.
pub async fn index_semantic(
    project_root: &Path,
    config: &Config,
    query: &str,
    top_k: usize,
    json: bool,
) -> Result<()> {
    if !config.vectors.enabled {
        return Err(HiefError::Config(
            "Semantic search is not enabled. Set vectors.enabled = true in hief.toml and rebuild the index.".to_string(),
        ));
    }

    let query_vector = crate::index::vectors::embed_text(query, config.vectors.dimensions)?;
    let semantic_query = crate::index::vectors::SemanticQuery {
        query: query.to_string(),
        top_k,
        language: None,
    };
    let results = crate::index::vectors::search(
        project_root,
        &query_vector,
        &semantic_query,
        &config.vectors,
    )
    .await?;

    print_semantic_results(query, &results, json);

    Ok(())
}

fn print_semantic_results(
    query: &str,
    results: &[crate::index::vectors::SemanticResult],
    json: bool,
) {
    print!("{}", render_semantic_results(query, results, json));
}

fn render_semantic_results(
    query: &str,
    results: &[crate::index::vectors::SemanticResult],
    json: bool,
) -> String {
    if json {
        format!("{}\n", serde_json::to_string_pretty(results).unwrap())
    } else if results.is_empty() {
        format!("No semantic results found for '{}'\n", query)
    } else {
        let mut output = format!("Found {} semantic results for '{}':\n\n", results.len(), query);
        for (index, result) in results.iter().enumerate() {
            let symbol = result.symbol_name.as_deref().unwrap_or("(anonymous)");
            output.push_str(&format!(
                "  {}. {} [score {:.3}] — {}:{}–{}\n",
                index + 1,
                symbol,
                result.score,
                result.file_path,
                result.start_line,
                result.end_line,
            ));
            for line in result.content.lines().take(3) {
                output.push_str(&format!("     {}\n", line));
            }
            output.push('\n');
        }
        output
    }
}

#[cfg(test)]
mod tests {
    use super::render_semantic_results;

    #[test]
    fn test_render_semantic_results_json() {
        let results = vec![crate::index::vectors::SemanticResult {
            chunk_id: "chunk-1".to_string(),
            file_path: "src/main.rs".to_string(),
            symbol_name: Some("main".to_string()),
            parent_scope: None,
            language: "rust".to_string(),
            content: "fn main() {}".to_string(),
            start_line: 1,
            end_line: 1,
            score: 0.91,
        }];

        let json = render_semantic_results("main", &results, true);
        assert!(json.contains("\"chunk_id\": \"chunk-1\""));
        assert!(json.contains("\"score\": 0.91"));
    }

    #[test]
    fn test_render_semantic_results_human() {
        let results = vec![crate::index::vectors::SemanticResult {
            chunk_id: "chunk-1".to_string(),
            file_path: "src/main.rs".to_string(),
            symbol_name: Some("main".to_string()),
            parent_scope: None,
            language: "rust".to_string(),
            content: "fn main() {}".to_string(),
            start_line: 1,
            end_line: 1,
            score: 0.91,
        }];

        let output = render_semantic_results("main", &results, false);
        assert!(output.contains("Found 1 semantic results for 'main'"));
        assert!(output.contains("1. main [score 0.910]"));
        assert!(output.contains("src/main.rs:1–1"));
    }
}
