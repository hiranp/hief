use askama::Template;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse};

use crate::errors::HiefError;
use crate::graph;
use crate::ui::view_models::{DashboardView, IntentRow, WorktreeRow};
use crate::ui::{worktree_git, UiState};

#[derive(Template)]
#[template(path = "dashboard.html")]
struct DashboardTemplate {
    view: DashboardView,
}

pub async fn dashboard(State(state): State<UiState>) -> impl IntoResponse {
    let intents = match graph::list_intents(&state.db, None, None).await {
        Ok(rows) => rows,
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("failed to load intents: {}", err),
            )
                .into_response();
        }
    };

    let intent_rows = intents
        .into_iter()
        .map(|intent| IntentRow {
            id: intent.id,
            title: intent.title,
            status: intent.status,
            priority: intent.priority,
            assigned_to: intent
                .assigned_to
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| "unassigned".to_string()),
            updated_at: intent.updated_at,
        })
        .collect();

    let worktree_rows = match worktree_git::list_worktrees(&state.project_root).await {
        Ok(rows) => rows
            .into_iter()
            .map(|row| WorktreeRow {
                path: row.path,
                head: row.head,
                branch: row.branch,
                locked: row.locked,
                prunable: row.prunable,
            })
            .collect(),
        Err(_) => Vec::new(),
    };

    let template = DashboardTemplate {
        view: DashboardView {
            intents: intent_rows,
            worktrees: worktree_rows,
        },
    };

    match template
        .render()
        .map_err(|e| HiefError::Other(format!("template render failed: {}", e)))
    {
        Ok(html) => Html(html).into_response(),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("failed to render dashboard: {}", err),
        )
            .into_response(),
    }
}
