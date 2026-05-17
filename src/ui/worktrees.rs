use std::collections::HashMap;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use serde::{Deserialize, Serialize};

use crate::scope;
use crate::ui::UiState;
use crate::ui::worktree_git;

#[derive(Debug, Clone, Serialize)]
pub struct WorktreeBindingRow {
    pub worktree_id: String,
    pub path: String,
    pub branch: Option<String>,
    pub locked: bool,
    pub prunable: bool,
    pub owner_intent_id: Option<String>,
    pub owner_holder: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct RemoveRequest {
    pub path: String,
    #[serde(default)]
    pub force: bool,
}

#[derive(Debug, Deserialize)]
pub struct CreateRequest {
    pub path: String,
    pub branch: String,
}

pub async fn list_worktrees(State(state): State<UiState>) -> impl IntoResponse {
    let list = match worktree_git::list_worktrees(&state.project_root).await {
        Ok(rows) => rows,
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": err.to_string()})),
            )
                .into_response();
        }
    };

    let lock_map = load_lock_index(&state).await.unwrap_or_default();
    let rows = list
        .into_iter()
        .map(|row| {
            let worktree_id = scope::derive_worktree_id(std::path::Path::new(&row.path));
            let owner = lock_map.get(&worktree_id).cloned();
            WorktreeBindingRow {
                worktree_id,
                path: row.path,
                branch: row.branch,
                locked: row.locked,
                prunable: row.prunable,
                owner_intent_id: owner.as_ref().map(|(intent_id, _)| intent_id.clone()),
                owner_holder: owner.map(|(_, holder)| holder),
            }
        })
        .collect::<Vec<_>>();

    Json(rows).into_response()
}

pub async fn create_worktree(
    State(state): State<UiState>,
    Json(req): Json<CreateRequest>,
) -> impl IntoResponse {
    if req.branch.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"ok": false, "reason": "branch is required"})),
        )
            .into_response();
    }

    let path = match worktree_git::join_worktree_path(&state.project_root, &req.path) {
        Ok(path) => path,
        Err(err) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"ok": false, "reason": err.to_string()})),
            )
                .into_response();
        }
    };

    match worktree_git::create_worktree(&state.project_root, &path, req.branch.trim()).await {
        Ok(()) => Json(serde_json::json!({"ok": true, "path": path, "branch": req.branch}))
            .into_response(),
        Err(err) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"ok": false, "reason": err.to_string()})),
        )
            .into_response(),
    }
}

pub async fn remove_worktree(
    State(state): State<UiState>,
    Json(req): Json<RemoveRequest>,
) -> impl IntoResponse {
    let path = match worktree_git::join_worktree_path(&state.project_root, &req.path) {
        Ok(path) => path,
        Err(err) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"ok": false, "reason": err.to_string()})),
            )
                .into_response();
        }
    };

    match worktree_git::remove_worktree(&state.project_root, &path, req.force).await {
        Ok(()) => Json(serde_json::json!({"ok": true, "path": path})).into_response(),
        Err(err) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"ok": false, "reason": err.to_string()})),
        )
            .into_response(),
    }
}

pub async fn lock_worktree(
    State(state): State<UiState>,
    Path(path): Path<String>,
) -> impl IntoResponse {
    let target = match worktree_git::join_worktree_path(&state.project_root, &path) {
        Ok(path) => path,
        Err(err) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"ok": false, "reason": err.to_string()})),
            )
                .into_response();
        }
    };
    match worktree_git::lock_worktree(&state.project_root, &target, "locked from ui").await {
        Ok(()) => Json(serde_json::json!({"ok": true})).into_response(),
        Err(err) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"ok": false, "reason": err.to_string()})),
        )
            .into_response(),
    }
}

pub async fn prune_worktrees(State(state): State<UiState>) -> impl IntoResponse {
    match worktree_git::prune_worktrees(&state.project_root).await {
        Ok(()) => Json(serde_json::json!({"ok": true})).into_response(),
        Err(err) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"ok": false, "reason": err.to_string()})),
        )
            .into_response(),
    }
}

pub async fn repair_worktrees(State(state): State<UiState>) -> impl IntoResponse {
    match worktree_git::repair_worktrees(&state.project_root).await {
        Ok(()) => Json(serde_json::json!({"ok": true})).into_response(),
        Err(err) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"ok": false, "reason": err.to_string()})),
        )
            .into_response(),
    }
}

async fn load_lock_index(
    state: &UiState,
) -> crate::errors::Result<HashMap<String, (String, String)>> {
    let mut rows = state
        .db
        .conn()
        .query(
            "SELECT intent_id, holder, worktree_id FROM intent_locks \
             WHERE expires_at > unixepoch()",
            (),
        )
        .await
        .map_err(crate::errors::HiefError::Database)?;

    let mut out = HashMap::new();
    while let Some(row) = rows
        .next()
        .await
        .map_err(crate::errors::HiefError::Database)?
    {
        let intent_id: String = row.get(0).map_err(crate::errors::HiefError::Database)?;
        let holder: String = row.get(1).map_err(crate::errors::HiefError::Database)?;
        let worktree_id: String = row.get(2).map_err(crate::errors::HiefError::Database)?;
        out.insert(worktree_id, (intent_id, holder));
    }
    Ok(out)
}
