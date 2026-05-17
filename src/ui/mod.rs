use std::path::PathBuf;

use axum::Router;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use tower_http::trace::TraceLayer;

use crate::config::Config;
use crate::db::Database;
use crate::errors::{HiefError, Result};

pub mod activity;
pub mod detail;
pub mod events;
pub mod pages;
pub mod review;
pub mod view_models;
pub mod worktree_git;
pub mod worktrees;

#[derive(Clone)]
pub struct UiState {
    pub db: Database,
    pub project_root: PathBuf,
    pub config: Config,
}

pub fn build_router(state: UiState) -> Router {
    Router::new()
        .route("/ui", get(pages::dashboard))
        .route("/ui/events", get(events::stream_events))
        .route("/ui/activity", get(activity::activity_fragment))
        .route("/ui/worktrees", get(worktrees::list_worktrees))
        .route("/ui/worktrees/create", post(worktrees::create_worktree))
        .route("/ui/worktrees/remove", post(worktrees::remove_worktree))
        .route("/ui/worktrees/{path}/lock", post(worktrees::lock_worktree))
        .route("/ui/worktrees/prune", post(worktrees::prune_worktrees))
        .route("/ui/worktrees/repair", post(worktrees::repair_worktrees))
        .route("/ui/tasks/{id}", get(detail::task_detail))
        .route("/ui/review/{id}", get(review::review_panel))
        .route("/ui/review/{id}/block", post(review::block_intent))
        .route("/ui/review/{id}/unblock", post(review::unblock_intent))
        .route("/ui/review/{id}/to-review", post(review::move_to_review))
        .route("/health", get(|| async { "ok" }))
        .fallback(get(not_found))
        .with_state(state)
        .layer(TraceLayer::new_for_http())
}

pub async fn start(db: Database, project_root: PathBuf, host: &str, port: u16) -> Result<()> {
    let config = Config::load(&project_root.join("hief.toml"))?;
    let state = UiState {
        db,
        project_root,
        config,
    };
    let app = build_router(state);

    let addr = format!("{}:{}", host, port);
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .map_err(|e| HiefError::Other(format!("failed to bind {}: {}", addr, e)))?;

    tracing::info!("UI server listening on http://{}", addr);
    axum::serve(listener, app)
        .await
        .map_err(|e| HiefError::Other(format!("ui server error: {}", e)))?;

    Ok(())
}

async fn not_found(State(_state): State<UiState>) -> impl IntoResponse {
    (StatusCode::NOT_FOUND, "Not found")
}
