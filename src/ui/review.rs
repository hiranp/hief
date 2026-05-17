use askama::Template;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};

use crate::scope;
use crate::ui::UiState;

#[derive(Template)]
#[template(path = "review_panel.html")]
struct ReviewPanelTemplate {
    intent_id: String,
    gate_state: String,
    gate_reason: String,
    total_calls: i64,
    avg_groundedness: String,
    message: Option<String>,
}

pub async fn review_panel(Path(id): Path<String>, State(state): State<UiState>) -> Response {
    render_panel(&state, &id, None).await
}

pub async fn block_intent(Path(id): Path<String>, State(state): State<UiState>) -> Response {
    transition(&state, &id, "blocked", "blocked from ui").await
}

pub async fn unblock_intent(Path(id): Path<String>, State(state): State<UiState>) -> Response {
    transition(&state, &id, "approved", "unblocked from ui").await
}

pub async fn move_to_review(Path(id): Path<String>, State(state): State<UiState>) -> Response {
    transition(&state, &id, "in_review", "moved to review from ui").await
}

async fn transition(state: &UiState, id: &str, to: &str, success_message: &str) -> Response {
    let worktree_id = scope::derive_worktree_id(&state.project_root);
    match crate::graph::update_status_scoped(
        &state.db,
        id,
        to,
        Some("ui"),
        Some(&worktree_id),
        state.config.graph.stale_timeout_hours,
    )
    .await
    {
        Ok(()) => render_panel(state, id, Some(success_message.to_string())).await,
        Err(err) => (
            StatusCode::BAD_REQUEST,
            Html(format!(
                "<div data-result='error' data-reason='{}'>transition rejected: {}</div>",
                err,
                err
            )),
        )
            .into_response(),
    }
}

async fn render_panel(state: &UiState, id: &str, message: Option<String>) -> Response {
    let health = crate::mcp::resources::get_project_health(&state.db, &state.project_root, &state.config)
        .await
        .ok();
    let gate_state = if health.as_ref().is_some_and(|h| h.wave_gate_open) {
        "open".to_string()
    } else {
        "blocked".to_string()
    };
    let gate_reason = health
        .as_ref()
        .and_then(|h| h.gate_reason.clone())
        .unwrap_or_else(|| "pass".to_string());

    let worktree_id = scope::derive_worktree_id(&state.project_root);
    let summary = state
        .db
        .get_session_cost_summary_scoped("ui-task-detail", Some(&worktree_id))
        .await
        .ok();

    let tpl = ReviewPanelTemplate {
        intent_id: id.to_string(),
        gate_state,
        gate_reason,
        total_calls: summary.as_ref().map_or(0, |s| s.total_calls),
        avg_groundedness: summary
            .and_then(|s| s.avg_groundedness)
            .map(|v| format!("{:.3}", v))
            .unwrap_or_else(|| "n/a".to_string()),
        message,
    };

    Html(tpl.render().unwrap_or_default()).into_response()
}
