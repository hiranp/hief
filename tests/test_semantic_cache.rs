use hief::config::Config;
use hief::db::Database;
use hief::index::vectors::{SemanticQuery, VectorConfig, search};
use tempfile::TempDir;

fn write_file(path: &std::path::Path, content: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("create parent dirs");
    }
    std::fs::write(path, content).expect("write file");
}

async fn prepare_vector_store(root: &std::path::Path) -> Database {
    write_file(
        &root.join("hief.toml"),
        r#"
[hief]
version = "0.2.8"
"#,
    );

    write_file(
        &root.join("src/lib.rs"),
        r#"
pub fn alpha() -> &'static str { "alpha" }
pub fn beta() -> &'static str { "beta" }
"#,
    );
    write_file(
        &root.join("src/helper.py"),
        r#"
def alpha():
    return "alpha"
"#,
    );

    let db = Database::open(&Config::db_path(root))
        .await
        .expect("open db");

    let config = VectorConfig {
        enabled: true,
        dimensions: 384,
        metric: "cosine".to_string(),
    };
    assert!(config.enabled, "test must explicitly enable vectors");

    let config_file = Config::load(&root.join("hief.toml")).expect("config");
    hief::index::build(&db, root, &config_file.index, &config)
        .await
        .expect("build index");

    db
}

#[tokio::test]
async fn test_semantic_cache_hit_miss_and_expiry() {
    let tmp = TempDir::new().expect("tempdir");
    let root = tmp.path();
    let db = prepare_vector_store(root).await;

    let config = VectorConfig {
        enabled: true,
        dimensions: 384,
        metric: "cosine".to_string(),
    };

    let query_vector =
        hief::index::vectors::embed_text("alpha function", config.dimensions).expect("embed query");
    let query = SemanticQuery {
        query: "alpha function".to_string(),
        top_k: 5,
        language: Some("rust".to_string()),
    };

    let first = search(&db, root, &query_vector, &query, &config)
        .await
        .expect("first search");
    assert!(
        !first.results.is_empty(),
        "expected at least one semantic result"
    );
    assert!(!first.cache_used, "first lookup should miss cache");

    let second = search(&db, root, &query_vector, &query, &config)
        .await
        .expect("second search");
    assert_eq!(
        first.results.len(),
        second.results.len(),
        "cache hit should preserve result count"
    );
    assert!(second.cache_used, "second lookup should hit cache");

    let mut rows = db
        .conn()
        .query(
            "SELECT query_fingerprint, language_scope, expires_at FROM semantic_cache LIMIT 1",
            (),
        )
        .await
        .expect("query cache row");
    let row = rows.next().await.expect("row result").expect("row");
    let fingerprint: String = row.get(0).expect("fingerprint");
    let language_scope: String = row.get(1).expect("language scope");
    let expires_at: i64 = row.get(2).expect("expires_at");
    assert!(!fingerprint.is_empty());
    assert_eq!(language_scope, "rust");
    assert!(expires_at > 0);

    db.conn()
        .execute("UPDATE semantic_cache SET expires_at = unixepoch() - 1", ())
        .await
        .expect("expire cache row");

    let third = search(&db, root, &query_vector, &query, &config)
        .await
        .expect("third search");
    assert_eq!(
        first.results.len(),
        third.results.len(),
        "expired cache should recompute deterministically"
    );
    assert!(
        !third.cache_used,
        "expired cache should fall back to recomputation"
    );
}

#[tokio::test]
async fn test_semantic_cache_separates_language_scope() {
    let tmp = TempDir::new().expect("tempdir");
    let root = tmp.path();
    let db = prepare_vector_store(root).await;

    let config = VectorConfig {
        enabled: true,
        dimensions: 384,
        metric: "cosine".to_string(),
    };

    let query_vector =
        hief::index::vectors::embed_text("alpha function", config.dimensions).expect("embed query");

    let rust_query = SemanticQuery {
        query: "alpha function".to_string(),
        top_k: 5,
        language: Some("rust".to_string()),
    };
    let python_query = SemanticQuery {
        query: "alpha function".to_string(),
        top_k: 5,
        language: Some("python".to_string()),
    };

    let rust_outcome = search(&db, root, &query_vector, &rust_query, &config)
        .await
        .expect("rust search");
    let python_outcome = search(&db, root, &query_vector, &python_query, &config)
        .await
        .expect("python search");
    assert!(!rust_outcome.cache_used);
    assert!(!python_outcome.cache_used);

    let mut rows = db
        .conn()
        .query("SELECT COUNT(*) FROM semantic_cache", ())
        .await
        .expect("count rows");
    let row = rows.next().await.expect("row result").expect("row");
    let count: i64 = row.get(0).expect("count");
    assert!(
        count >= 2,
        "language scopes should map to separate cache rows"
    );
}
