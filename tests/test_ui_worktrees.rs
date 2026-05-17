use std::fs;
use std::path::PathBuf;

use axum::body::{to_bytes, Body};
use axum::http::{Method, Request, StatusCode};
use hief::config::Config;
use hief::db::Database;
use hief::ui::{self, UiState};
use hief::ui::worktree_git;
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

#[test]
fn test_porcelain_parser_extracts_fields() {
    let sample = "worktree /tmp/repo\nHEAD 1234\nbranch refs/heads/main\nlocked\n\n\
                  worktree /tmp/repo2\nHEAD 5678\nprunable\n";
    let parsed = worktree_git::parse_porcelain(sample).expect("parse");
    assert_eq!(parsed.len(), 2);
    assert_eq!(parsed[0].path, "/tmp/repo");
    assert_eq!(parsed[0].branch.as_deref(), Some("main"));
    assert!(parsed[0].locked);
    assert!(parsed[1].prunable);
}

#[tokio::test]
async fn test_worktree_list_route_returns_json() {
    let (_dir, state) = test_state().await;
    let app = ui::build_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/ui/worktrees")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("body");
    let text = String::from_utf8(body.to_vec()).expect("utf8");
    assert!(text.starts_with("["));
    assert!(text.contains("worktree_id"));
}

#[tokio::test]
async fn test_remove_worktree_rejects_dirty_tree_without_force() {
    let dir = tempfile::tempdir().expect("tempdir");
    std::process::Command::new("git")
        .arg("init")
        .arg("-q")
        .current_dir(dir.path())
        .status()
        .expect("git init");
    fs::write(dir.path().join("dirty.txt"), "changed").expect("write dirty");

    let err = worktree_git::remove_worktree(dir.path(), dir.path(), false)
        .await
        .expect_err("dirty tree should fail");
    assert!(err.to_string().contains("dirty worktree"));
}

#[test]
fn test_join_worktree_path_rejects_absolute_path_outside_project() {
    let dir = tempfile::tempdir().expect("tempdir");
    let outside = if cfg!(windows) {
        PathBuf::from("C:/outside")
    } else {
        PathBuf::from("/tmp")
    };

    let err = worktree_git::join_worktree_path(
        dir.path(),
        outside.to_str().expect("outside path utf8"),
    )
    .expect_err("absolute outside path should fail");

    assert!(err.to_string().contains("path traversal"));
}

#[test]
fn test_join_worktree_path_rejects_parent_traversal() {
    let dir = tempfile::tempdir().expect("tempdir");
    let err = worktree_git::join_worktree_path(dir.path(), "../../etc")
        .expect_err("parent traversal should fail");
    assert!(err.to_string().contains("path traversal"));
}

#[tokio::test]
async fn test_mutating_worktree_routes_are_registered() {
    let (_dir, state) = test_state().await;
    let app = ui::build_router(state);

    let remove_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/ui/worktrees/remove")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"path":"/tmp","force":false}"#))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_ne!(remove_response.status(), StatusCode::NOT_FOUND);

    let lock_response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/ui/worktrees/example/lock")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_ne!(lock_response.status(), StatusCode::NOT_FOUND);
}
