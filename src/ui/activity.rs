use askama::Template;
use axum::extract::State;
use axum::response::{Html, IntoResponse};

use crate::errors::{HiefError, Result};
use crate::scope;
use crate::ui::view_models::ActivityRow;
use crate::ui::UiState;

#[derive(Template)]
#[template(path = "activity_feed.html")]
struct ActivityFeedTemplate {
    rows: Vec<ActivityRow>,
    reconnect_refresh: bool,
}

pub async fn load_recent_activity(state: &UiState, limit: usize) -> Result<Vec<ActivityRow>> {
    let worktree_id = scope::derive_worktree_id(&state.project_root);
    let sql = "SELECT query, tool, created_at, latency_ms, groundedness_score FROM tool_events WHERE worktree_id = ?1 ORDER BY created_at DESC, id DESC LIMIT ?2";

    let mut rows = state
        .db
        .conn()
        .query(sql, libsql::params![worktree_id.as_str(), limit as i64])
        .await
        .map_err(HiefError::Database)?;

    let mut output = Vec::new();
    while let Some(row) = rows.next().await.map_err(HiefError::Database)? {
        let query: String = row.get(0).map_err(HiefError::Database)?;
        let tool: String = row.get(1).map_err(HiefError::Database)?;
        let created_at: i64 = row.get(2).map_err(HiefError::Database)?;
        let latency_ms = row.get::<Option<i64>>(3).map_err(HiefError::Database)?;
        let groundedness = row.get::<Option<f64>>(4).map_err(HiefError::Database)?;
        output.push(ActivityRow {
            intent_id: infer_intent_id(&query),
            tool,
            created_at,
            latency_ms,
            groundedness,
        });
    }

    Ok(output)
}

pub async fn activity_fragment(State(state): State<UiState>) -> impl IntoResponse {
    let rows = load_recent_activity(&state, 20).await.unwrap_or_default();
    let tpl = ActivityFeedTemplate {
        rows,
        reconnect_refresh: true,
    };

    match tpl
        .render()
        .map_err(|e| HiefError::Other(format!("activity render failed: {}", e)))
    {
        Ok(html) => Html(html).into_response(),
        Err(err) => Html(format!("<div class=\"error\">{}</div>", err)).into_response(),
    }
}

fn infer_intent_id(query: &str) -> Option<String> {
    query
        .split_whitespace()
        .find(|token| token.starts_with("hief-"))
        .map(ToString::to_string)
}
