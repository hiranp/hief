//! Tests for tool event telemetry and session summaries.

use hief::db::Database;
use tempfile::TempDir;

async fn open_test_db() -> Database {
    let temp = TempDir::new().expect("create temp dir");
    let db_path = temp.path().join("test.db");
    Database::open(&db_path).await.expect("open db")
}

#[tokio::test]
async fn test_tool_events_migration_creates_table() {
    let db = open_test_db().await;

    // Verify tool_events table exists
    let mut rows = db
        .conn()
        .query(
            "SELECT name FROM sqlite_master WHERE type='table' AND name='tool_events'",
            (),
        )
        .await
        .expect("query schema");

    let row = rows
        .next()
        .await
        .expect("row result")
        .expect("tool_events table should exist");
    let table_name: String = row.get(0).expect("table name");
    assert_eq!(table_name, "tool_events");
}

#[tokio::test]
async fn test_tool_events_migration_creates_session_summary_view() {
    let db = open_test_db().await;

    // Verify session_summary view exists
    let mut rows = db
        .conn()
        .query(
            "SELECT name FROM sqlite_master WHERE type='view' AND name='session_summary'",
            (),
        )
        .await
        .expect("query schema");

    let row = rows
        .next()
        .await
        .expect("row result")
        .expect("session_summary view should exist");
    let view_name: String = row.get(0).expect("view name");
    assert_eq!(view_name, "session_summary");
}

#[tokio::test]
async fn test_migration_order_is_valid() {
    let db = open_test_db().await;

    // Verify all migrations in order
    let mut rows = db
        .conn()
        .query("SELECT name FROM _migrations ORDER BY id", ())
        .await
        .expect("query migrations");

    let mut names = Vec::new();
    while let Some(row) = rows.next().await.expect("row result") {
        let name: String = row.get(0).expect("migration name");
        names.push(name);
    }

    assert!(
        names.contains(&"006_tool_events".to_string()),
        "006_tool_events migration should be applied"
    );

    // Verify 006 comes after 005
    let pos_005 = names.iter().position(|n| n == "005_semantic_cache");
    let pos_006 = names.iter().position(|n| n == "006_tool_events");
    assert!(pos_005.is_some() && pos_006.is_some());
    assert!(
        pos_005.unwrap() < pos_006.unwrap(),
        "005 should come before 006"
    );
}

#[tokio::test]
async fn test_record_tool_event_with_all_fields() {
    let db = open_test_db().await;

    let event_id = db
        .record_tool_event_scoped(
            "session-123",
            "search_code",
            "find authentication",
            Some("semantic"),
            Some(42),
            Some(125),
            Some(0.95),
            None,
        )
        .await
        .expect("record event");

    assert!(event_id > 0, "event_id should be positive");

    // Verify event was inserted
    let mut rows = db
        .conn()
        .query(
            "SELECT session_id, tool, query, strategy, result_count, latency_ms, \
             groundedness_score FROM tool_events WHERE id = ?1",
            [event_id.to_string()],
        )
        .await
        .expect("query event");

    let row = rows
        .next()
        .await
        .expect("row result")
        .expect("event should exist");

    let session_id: String = row.get(0).expect("session_id");
    let tool: String = row.get(1).expect("tool");
    let query: String = row.get(2).expect("query");
    let strategy: String = row.get(3).expect("strategy");
    let result_count: i32 = row.get(4).expect("result_count");
    let latency_ms: i32 = row.get(5).expect("latency_ms");
    let groundedness_score: f64 = row.get(6).expect("groundedness_score");

    assert_eq!(session_id, "session-123");
    assert_eq!(tool, "search_code");
    assert_eq!(query, "find authentication");
    assert_eq!(strategy, "semantic");
    assert_eq!(result_count, 42);
    assert_eq!(latency_ms, 125);
    assert_eq!(groundedness_score, 0.95);
}

#[tokio::test]
async fn test_record_tool_event_with_optional_fields_none() {
    let db = open_test_db().await;

    let event_id = db
        .record_tool_event_scoped(
            "session-456",
            "search_semantic",
            "database connection",
            None,
            None,
            None,
            None,
            None,
        )
        .await
        .expect("record event");

    assert!(event_id > 0);

    // Verify event was inserted with NULL optional fields
    let mut rows = db
        .conn()
        .query(
            "SELECT session_id, tool, query, strategy, result_count, latency_ms, \
             groundedness_score FROM tool_events WHERE id = ?1",
            [event_id.to_string()],
        )
        .await
        .expect("query event");

    let row = rows
        .next()
        .await
        .expect("row result")
        .expect("event should exist");

    let session_id: String = row.get(0).expect("session_id");
    let tool: String = row.get(1).expect("tool");
    let query: String = row.get(2).expect("query");

    assert_eq!(session_id, "session-456");
    assert_eq!(tool, "search_semantic");
    assert_eq!(query, "database connection");

    // NULL values should not panic when not cast
}

#[tokio::test]
async fn test_session_summary_aggregation() {
    let db = open_test_db().await;

    let session_id = "session-agg-test";

    // Insert multiple events for the same session
    db.record_tool_event_scoped(
        session_id,
        "search_code",
        "auth",
        Some("deterministic"),
        Some(10),
        Some(50),
        Some(0.90),
        None,
    )
    .await
    .expect("record event 1");

    db.record_tool_event_scoped(
        session_id,
        "search_semantic",
        "database",
        Some("semantic"),
        Some(15),
        Some(100),
        Some(0.85),
        None,
    )
    .await
    .expect("record event 2");

    db.record_tool_event_scoped(
        session_id,
        "search_code",
        "routing",
        Some("deterministic"),
        Some(20),
        Some(75),
        Some(0.92),
        None,
    )
    .await
    .expect("record event 3");

    // Query session summary
    let summary = db
        .get_session_summary_scoped(session_id, None)
        .await
        .expect("get summary")
        .expect("summary should exist");

    assert_eq!(summary.session_id, session_id);
    assert_eq!(summary.total_events, 3);
    assert_eq!(summary.unique_tools, 2); // search_code and search_semantic
    assert_eq!(summary.avg_results, Some((10.0 + 15.0 + 20.0) / 3.0));
    assert_eq!(summary.avg_latency_ms, Some((50.0 + 100.0 + 75.0) / 3.0));

    let avg_groundedness = summary.avg_groundedness.expect("avg groundedness");
    let expected_groundedness = (0.90 + 0.85 + 0.92) / 3.0;
    assert!(
        (avg_groundedness - expected_groundedness).abs() < 0.01,
        "groundedness should match within 0.01"
    );

    assert!(summary.session_start > 0);
    assert!(summary.session_end >= summary.session_start);
    assert!(summary.session_duration_seconds >= 0);
}

#[tokio::test]
async fn test_session_summary_nonexistent_session() {
    let db = open_test_db().await;

    let summary = db
        .get_session_summary_scoped("nonexistent-session", None)
        .await
        .expect("get summary");

    assert!(summary.is_none(), "nonexistent session should return None");
}

#[tokio::test]
async fn test_tool_events_roundtrip_with_multiple_sessions() {
    let db = open_test_db().await;

    // Insert events for multiple sessions
    db.record_tool_event_scoped(
        "session-1",
        "search_code",
        "query1",
        Some("deterministic"),
        Some(5),
        Some(10),
        Some(0.80),
        None,
    )
    .await
    .expect("record 1");

    db.record_tool_event_scoped(
        "session-1",
        "search_code",
        "query2",
        Some("deterministic"),
        Some(8),
        Some(15),
        Some(0.85),
        None,
    )
    .await
    .expect("record 2");

    db.record_tool_event_scoped(
        "session-2",
        "search_semantic",
        "query3",
        Some("semantic"),
        Some(12),
        Some(50),
        Some(0.90),
        None,
    )
    .await
    .expect("record 3");

    // Verify session-1 summary
    let summary1 = db
        .get_session_summary_scoped("session-1", None)
        .await
        .expect("get summary 1")
        .expect("summary 1 should exist");

    assert_eq!(summary1.total_events, 2);
    assert_eq!(summary1.unique_tools, 1);

    // Verify session-2 summary
    let summary2 = db
        .get_session_summary_scoped("session-2", None)
        .await
        .expect("get summary 2")
        .expect("summary 2 should exist");

    assert_eq!(summary2.total_events, 1);
    assert_eq!(summary2.unique_tools, 1);
}

#[tokio::test]
async fn test_session_cost_summary_includes_totals_and_breakdown() {
    let db = open_test_db().await;

    db.record_tool_event_scoped(
        "session-cost-1",
        "search_code",
        "auth",
        Some("strategy=deterministic;lane=mcp;reason=test;outcome=ok"),
        Some(2),
        Some(30),
        Some(0.8),
        None,
    )
    .await
    .expect("insert event 1");
    db.record_tool_event_scoped(
        "session-cost-1",
        "search_code",
        "authz",
        Some("strategy=deterministic;lane=mcp;reason=test;outcome=ok"),
        Some(3),
        Some(70),
        Some(0.9),
        None,
    )
    .await
    .expect("insert event 2");
    db.record_tool_event_scoped(
        "session-cost-1",
        "semantic_search",
        "token budget",
        Some("strategy=semantic;lane=progressive_mcp;reason=test;outcome=ok"),
        Some(4),
        Some(50),
        Some(0.7),
        None,
    )
    .await
    .expect("insert event 3");

    let summary = db
        .get_session_cost_summary_scoped("session-cost-1", None)
        .await
        .expect("session cost summary");

    assert_eq!(summary.total_calls, 3);
    assert_eq!(summary.total_latency_ms, 150);
    assert_eq!(summary.per_tool.len(), 2);

    let search_code = summary
        .per_tool
        .iter()
        .find(|row| row.tool == "search_code")
        .expect("search_code row");
    assert_eq!(search_code.total_calls, 2);
    assert_eq!(search_code.total_latency_ms, 100);
}

#[tokio::test]
async fn test_session_cost_summary_empty_session_returns_zero_values() {
    let db = open_test_db().await;

    let summary = db
        .get_session_cost_summary_scoped("missing-session", None)
        .await
        .expect("session cost summary");

    assert_eq!(summary.session_id, "missing-session");
    assert_eq!(summary.total_calls, 0);
    assert_eq!(summary.total_latency_ms, 0);
    assert!(summary.avg_groundedness.is_none());
    assert!(summary.per_tool.is_empty());
}
