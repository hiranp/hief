use std::convert::Infallible;

use axum::extract::State;
use axum::response::sse::{Event, KeepAlive, Sse};
use futures::stream;
use serde::Serialize;

use crate::scope;
use crate::ui::UiState;
use crate::ui::activity;

#[derive(Debug, Clone, Serialize)]
struct ActivityPayload {
    intent_id: Option<String>,
    tool: String,
    timestamp: i64,
    latency_ms: Option<i64>,
    quality: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
struct TaskPayload {
    intent_id: Option<String>,
    status: String,
    timestamp: i64,
}

pub async fn stream_events(
    State(state): State<UiState>,
) -> Sse<impl futures::Stream<Item = Result<Event, Infallible>>> {
    let _worktree_id = scope::derive_worktree_id(&state.project_root);
    let activity_rows = activity::load_recent_activity(&state, 5)
        .await
        .unwrap_or_default();

    let mut events = Vec::new();
    for row in activity_rows {
        let payload = ActivityPayload {
            intent_id: row.intent_id.clone(),
            tool: row.tool,
            timestamp: row.created_at,
            latency_ms: row.latency_ms,
            quality: row.groundedness,
        };
        if let Ok(data) = serde_json::to_string(&payload) {
            events.push(Ok(Event::default().event("activity-update").data(data)));
        }

        let task_payload = TaskPayload {
            intent_id: row.intent_id,
            status: "observed".to_string(),
            timestamp: row.created_at,
        };
        if let Ok(data) = serde_json::to_string(&task_payload) {
            events.push(Ok(Event::default().event("task-update").data(data)));
        }
    }

    if events.is_empty() {
        events.push(Ok(Event::default().event("activity-update").data("{}")));
    }

    Sse::new(stream::iter(events))
        .keep_alive(KeepAlive::new().interval(std::time::Duration::from_secs(15)))
}
