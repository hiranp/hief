use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use hief::config::Config;
use hief::db::Database;
use hief::ui::{self, UiState};
use tower::util::ServiceExt;

async fn test_state() -> (tempfile::TempDir, UiState) {
    let dir = tempfile::tempdir().expect("tempdir");
    let project_root = dir.path().to_path_buf();
    std::process::Command::new("git")
        .arg("init")
        .arg("-q")
        .current_dir(&project_root)
        .status()
        .expect("git init");

    let db = Database::open(&project_root.join("hief.db"))
        .await
        .expect("open db");
    let worktree_id = hief::scope::derive_worktree_id(&project_root);
    db.record_tool_event_scoped(
        "ui-task-detail",
        "search_code",
        "hief-123 find records",
        Some("deterministic"),
        Some(3),
        Some(10),
        Some(0.8),
        Some(&worktree_id),
    )
    .await
    .expect("record event");

    (
        dir,
        UiState {
            db,
            project_root,
            config: Config::default(),
        },
    )
}

#[tokio::test]
async fn test_sse_endpoint_content_type_and_events() {
    let (_dir, state) = test_state().await;
    let app = ui::build_router(state);

    let response = app
        .oneshot(Request::builder().uri("/ui/events").body(Body::empty()).expect("request"))
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let content_type = response
        .headers()
        .get("content-type")
        .expect("content-type")
        .to_str()
        .expect("content-type str");
    assert!(content_type.starts_with("text/event-stream"));

    let body = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("body");
    let text = String::from_utf8(body.to_vec()).expect("utf8");
    assert!(text.contains("activity-update"));
    assert!(text.contains("task-update"));
    assert!(text.contains("latency_ms"));
}

#[tokio::test]
async fn test_activity_fragment_has_reconnect_refresh_hook() {
    let (_dir, state) = test_state().await;
    let app = ui::build_router(state);

    let response = app
        .oneshot(Request::builder().uri("/ui/activity").body(Body::empty()).expect("request"))
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("body");
    let html = String::from_utf8(body.to_vec()).expect("utf8");
    assert!(html.contains("sse-connect=\"/ui/events\""));
    assert!(html.contains("hx-trigger=\"sse:open from:body\""));
}
