use hief::db::Database;
use hief::eval::scorer::groundedness_score;
use hief::index::search::SearchQuery;
use tempfile::TempDir;

async fn open_test_db() -> Database {
    let temp = TempDir::new().expect("create temp dir");
    let db_path = temp.path().join("test.db");
    Database::open(&db_path).await.expect("open db")
}

#[test]
fn test_groundedness_score_is_deterministic_and_bounded() {
    let query = "authentication token refresh flow";
    let docs = vec![
        "refresh tokens and authentication lifecycle",
        "token rotation policy",
    ];

    let first = groundedness_score(query, &docs);
    let second = groundedness_score(query, &docs);

    assert!((0.0..=1.0).contains(&first));
    assert!((0.0..=1.0).contains(&second));
    assert_eq!(first, second);
}

#[test]
fn test_groundedness_score_handles_empty_signal() {
    let empty_docs: Vec<&str> = Vec::new();
    let score = groundedness_score("", &empty_docs);
    assert_eq!(score, 0.0);

    let low_signal = groundedness_score("opaque query", &["zzzz yyyy"]);
    assert!((0.0..=1.0).contains(&low_signal));
}

#[tokio::test]
async fn test_lexical_search_populates_groundedness_signal() {
    let db = open_test_db().await;

    db.conn()
        .execute(
            "INSERT INTO chunks (file_path, symbol_name, symbol_kind, parent_scope, language, content, start_line, end_line, content_hash)
             VALUES ('src/auth.rs', 'validate_token', 'fn', 'auth', 'rust', 'fn validate_token(token: &str) { refresh_token(token); }', 1, 10, 'hash-a')",
            (),
        )
        .await
        .expect("insert chunk 1");

    db.conn()
        .execute(
            "INSERT INTO chunks (file_path, symbol_name, symbol_kind, parent_scope, language, content, start_line, end_line, content_hash)
             VALUES ('src/cache.rs', 'warm_cache', 'fn', 'cache', 'rust', 'fn warm_cache() { preload(); }', 1, 10, 'hash-b')",
            (),
        )
        .await
        .expect("insert chunk 2");

    let mut query = SearchQuery::new("validate token refresh");
    query.top_k = 5;

    let results = hief::index::search(&db, &query).await.expect("search");
    assert!(!results.is_empty());

    let score = results[0].groundedness_score.expect("groundedness score");
    assert!((0.0..=1.0).contains(&score));
}

#[tokio::test]
async fn test_trajectory_row_persists_query_strategy_score_and_session() {
    let db = open_test_db().await;

    db.record_tool_event(
        "trajectory-session",
        "semantic_search",
        "auth refresh",
        Some("strategy=semantic;lane=mcp;reason=test;outcome=ok"),
        Some(6),
        Some(42),
        Some(0.77),
    )
    .await
    .expect("record trajectory event");

    let mut rows = db
        .conn()
        .query(
            "SELECT session_id, tool, query, strategy, result_count, groundedness_score
             FROM tool_events WHERE session_id = 'trajectory-session'",
            (),
        )
        .await
        .expect("query tool events");

    let row = rows
        .next()
        .await
        .expect("row result")
        .expect("event row");

    let session_id: String = row.get(0).expect("session_id");
    let tool: String = row.get(1).expect("tool");
    let query: String = row.get(2).expect("query");
    let strategy: String = row.get(3).expect("strategy");
    let result_count: i64 = row.get(4).expect("result_count");
    let groundedness_score: f64 = row.get(5).expect("groundedness_score");

    assert_eq!(session_id, "trajectory-session");
    assert_eq!(tool, "semantic_search");
    assert_eq!(query, "auth refresh");
    assert!(strategy.contains("lane=mcp"));
    assert_eq!(result_count, 6);
    assert!((groundedness_score - 0.77).abs() < 1e-9);
}
