use axum::body::{to_bytes, Body};
use axum::http::{Method, Request, StatusCode};
use hief::config::Config;
use hief::db::Database;
use hief::graph;
use hief::graph::Intent;
use hief::ui::{self, UiState};
use tower::util::ServiceExt;

async fn test_state() -> (tempfile::TempDir, UiState, String) {
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
    let intent = Intent::new("feature", "Detail page", None, None);
    let id = intent.id.clone();
    graph::create_intent(&db, &intent).await.expect("create intent");

    (
        dir,
        UiState {
            db,
            project_root,
            config: Config::default(),
        },
        id,
    )
}

#[tokio::test]
async fn test_task_detail_found_and_not_found_states() {
    let (_dir, state, id) = test_state().await;
    let app = ui::build_router(state.clone());

    let found = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/ui/tasks/{id}"))
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(found.status(), StatusCode::OK);
    let found_html = String::from_utf8(
        to_bytes(found.into_body(), 1024 * 1024)
            .await
            .expect("body")
            .to_vec(),
    )
    .expect("utf8");
    assert!(found_html.contains("PAVL Gate"));
    assert!(found_html.contains("Telemetry"));

    let missing = app
        .oneshot(
            Request::builder()
                .uri("/ui/tasks/does-not-exist")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(missing.status(), StatusCode::OK);
    let missing_html = String::from_utf8(
        to_bytes(missing.into_body(), 1024 * 1024)
            .await
            .expect("body")
            .to_vec(),
    )
    .expect("utf8");
    assert!(missing_html.contains("id=\"task-not-found\""));
}

#[tokio::test]
async fn test_review_transition_validation_rejects_invalid_move() {
    let (_dir, state, id) = test_state().await;
    let app = ui::build_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/ui/review/{id}/to-review"))
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = String::from_utf8(
        to_bytes(response.into_body(), 1024 * 1024)
            .await
            .expect("body")
            .to_vec(),
    )
    .expect("utf8");
    assert!(body.contains("transition rejected"));
    assert!(body.contains("invalid status transition"));
}
