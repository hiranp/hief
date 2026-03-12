//! `hief index` — code indexing and search commands.

use std::path::Path;

use crate::config::Config;
use crate::db::Database;
use crate::errors::Result;

/// Build or update the code index.
pub async fn index_build(
    db: &Database,
    project_root: &Path,
    config: &Config,
    json: bool,
) -> Result<()> {
    let report = crate::index::build(db, project_root, &config.index).await?;

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
    _project_root: &Path,
    query: &str,
    top_k: usize,
    json: bool,
) -> Result<()> {
    if json {
        println!(
            "{}",
            serde_json::json!({
                "error": "not_implemented",
                "message": "Semantic search is currently in development. Vector storage and retrieval are being implemented."
            })
        );
    } else {
        println!("⚠️  Semantic search is currently in development.");
        println!("   Vector storage and retrieval via LanceDB are being implemented.");
        println!(
            "   Query '{}' (top_k={}) would be executed here.",
            query, top_k
        );
    }

    Ok(())
}
