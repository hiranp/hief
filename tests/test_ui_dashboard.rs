use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use hief::config::Config;
use hief::db::Database;
use hief::graph;
use hief::graph::Intent;
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
async fn test_dashboard_route_200_with_empty_states() {
    let (_dir, state) = test_state().await;
    let app = ui::build_router(state);

    let response = app
        .oneshot(Request::builder().uri("/ui").body(Body::empty()).expect("request"))
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("body bytes");
    let html = String::from_utf8(body.to_vec()).expect("utf8");
    assert!(html.contains("id=\"intents-section\""));
    assert!(html.contains("id=\"worktrees-section\""));
    assert!(html.contains("id=\"intents-empty\""));
}

#[tokio::test]
async fn test_dashboard_renders_intent_rows() {
    let (_dir, state) = test_state().await;
    let intent = Intent::new("feature", "UI dashboard entry", None, None);
    let intent_id = intent.id.clone();
    graph::create_intent(&state.db, &intent).await.expect("create intent");

    let app = ui::build_router(state);
    let response = app
        .oneshot(Request::builder().uri("/ui").body(Body::empty()).expect("request"))
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);

    let html = String::from_utf8(
        to_bytes(response.into_body(), 1024 * 1024)
            .await
            .expect("body")
            .to_vec(),
    )
    .expect("utf8");
    assert!(html.contains(&intent_id));
    assert!(html.contains("UI dashboard entry"));
}
