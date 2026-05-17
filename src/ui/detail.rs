use askama::Template;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse};

use crate::mcp::resources;
use crate::scope;
use crate::ui::UiState;
use crate::ui::activity;

#[derive(Template)]
#[template(path = "task_detail.html")]
struct TaskDetailTemplate {
    found: bool,
    intent_id: String,
    title: String,
    status: String,
    priority: String,
    assigned_to: String,
    dependencies: Vec<String>,
    gate_open: bool,
    gate_reason: String,
    total_calls: i64,
    total_latency_ms: i64,
    groundedness: String,
    activity_count: usize,
}

pub async fn task_detail(
    Path(id): Path<String>,
    State(state): State<UiState>,
) -> impl IntoResponse {
    let intent = match crate::graph::get_intent_with_deps(&state.db, &id).await {
        Ok(found) => found,
        Err(_) => {
            let tpl = TaskDetailTemplate {
                found: false,
                intent_id: id,
                title: "Unknown task".to_string(),
                status: "not_found".to_string(),
                priority: "n/a".to_string(),
                assigned_to: "n/a".to_string(),
                dependencies: Vec::new(),
                gate_open: false,
                gate_reason: "intent_not_found".to_string(),
                total_calls: 0,
                total_latency_ms: 0,
                groundedness: "n/a".to_string(),
                activity_count: 0,
            };
            return match tpl.render() {
                Ok(html) => Html(html).into_response(),
                Err(err) => {
                    tracing::error!("task detail render failed (not found view): {}", err);
                    (StatusCode::INTERNAL_SERVER_ERROR, "render error").into_response()
                }
            };
        }
    };

    let health = resources::get_project_health(&state.db, &state.project_root, &state.config)
        .await
        .ok();
    let gate_open = health.as_ref().is_some_and(|h| h.wave_gate_open);
    let gate_reason = health
        .as_ref()
        .and_then(|h| h.gate_reason.clone())
        .unwrap_or_else(|| "pass".to_string());

    let worktree_id = scope::derive_worktree_id(&state.project_root);
    const UI_TASK_DETAIL_SESSION: &str = "ui-task-detail";
    let session_summary = state
        .db
        .get_session_cost_summary_scoped(UI_TASK_DETAIL_SESSION, Some(&worktree_id))
        .await
        .ok();
    let recent_activity = activity::load_recent_activity(&state, 20)
        .await
        .unwrap_or_default();

    let tpl = TaskDetailTemplate {
        found: true,
        intent_id: intent.intent.id,
        title: intent.intent.title,
        status: intent.intent.status,
        priority: intent.intent.priority,
        assigned_to: intent
            .intent
            .assigned_to
            .filter(|v| !v.trim().is_empty())
            .unwrap_or_else(|| "unassigned".to_string()),
        dependencies: intent.depends_on.into_iter().map(|dep| dep.id).collect(),
        gate_open,
        gate_reason,
        total_calls: session_summary.as_ref().map_or(0, |s| s.total_calls),
        total_latency_ms: session_summary.as_ref().map_or(0, |s| s.total_latency_ms),
        groundedness: session_summary
            .and_then(|s| s.avg_groundedness)
            .map(|v| format!("{:.3}", v))
            .unwrap_or_else(|| "n/a".to_string()),
        activity_count: recent_activity.len(),
    };

    match tpl.render() {
        Ok(html) => Html(html).into_response(),
        Err(err) => {
            tracing::error!("task detail render failed: {}", err);
            (StatusCode::INTERNAL_SERVER_ERROR, "render error").into_response()
        }
    }
}
