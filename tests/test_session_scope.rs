use hief::db::Database;
use hief::index::memory;

async fn open_test_db() -> (tempfile::TempDir, Database) {
    let dir = tempfile::tempdir().expect("create tempdir");
    let db_path = dir.path().join("hief.db");
    let db = Database::open(&db_path).await.expect("open db");
    (dir, db)
}

#[tokio::test]
async fn test_get_session_context_isolated_per_worktree() {
    let (_dir, db) = open_test_db().await;

    memory::record_access_scoped(
        &db,
        "src/worktree_a.rs",
        None,
        Some("query"),
        "search_code",
        Some("session-1"),
        Some("wt-a"),
    )
    .await
    .expect("record worktree A access");

    memory::record_access_scoped(
        &db,
        "src/worktree_b.rs",
        None,
        Some("query"),
        "search_code",
        Some("session-1"),
        Some("wt-b"),
    )
    .await
    .expect("record worktree B access");

    let ctx_a = memory::get_session_context_scoped(&db, "session-1", 10, Some("wt-a"))
        .await
        .expect("get session context A");
    let ctx_b = memory::get_session_context_scoped(&db, "session-1", 10, Some("wt-b"))
        .await
        .expect("get session context B");

    assert_eq!(ctx_a.accessed_files.len(), 1);
    assert_eq!(ctx_b.accessed_files.len(), 1);
    assert_eq!(ctx_a.accessed_files[0].file_path, "src/worktree_a.rs");
    assert_eq!(ctx_b.accessed_files[0].file_path, "src/worktree_b.rs");
}

#[tokio::test]
async fn test_session_summary_isolated_per_worktree() {
    let (_dir, db) = open_test_db().await;

    db.record_tool_event_scoped(
        "shared-session",
        "search_code",
        "auth",
        Some("strategy=deterministic"),
        Some(1),
        Some(10),
        Some(0.8),
        Some("wt-a"),
    )
    .await
    .expect("record event A");

    db.record_tool_event_scoped(
        "shared-session",
        "search_code",
        "auth",
        Some("strategy=deterministic"),
        Some(2),
        Some(20),
        Some(0.6),
        Some("wt-b"),
    )
    .await
    .expect("record event B");

    let summary_a = db
        .get_session_cost_summary_scoped("shared-session", Some("wt-a"))
        .await
        .expect("summary A");
    let summary_b = db
        .get_session_cost_summary_scoped("shared-session", Some("wt-b"))
        .await
        .expect("summary B");

    assert_eq!(summary_a.total_calls, 1);
    assert_eq!(summary_b.total_calls, 1);
    assert_eq!(summary_a.total_latency_ms, 10);
    assert_eq!(summary_b.total_latency_ms, 20);
}

#[tokio::test]
async fn test_default_scope_without_worktree_id_uses_project_root_bucket() {
    let (_dir, db) = open_test_db().await;

    memory::record_access_scoped(
        &db,
        "src/root_scope.rs",
        None,
        None,
        "search_code",
        Some("session-default"),
        None,
    )
    .await
    .expect("record default scope access");

    let ctx = memory::get_session_context_scoped(&db, "session-default", 10, None)
        .await
        .expect("get default scoped context");

    assert_eq!(ctx.accessed_files.len(), 1);
    assert_eq!(ctx.accessed_files[0].file_path, "src/root_scope.rs");
}
