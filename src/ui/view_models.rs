use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct DashboardView {
    pub intents: Vec<IntentRow>,
    pub worktrees: Vec<WorktreeRow>,
}

#[derive(Debug, Clone, Serialize)]
pub struct IntentRow {
    pub id: String,
    pub title: String,
    pub status: String,
    pub priority: String,
    pub assigned_to: String,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorktreeRow {
    pub path: String,
    pub head: Option<String>,
    pub branch: Option<String>,
    pub locked: bool,
    pub prunable: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ActivityRow {
    pub intent_id: Option<String>,
    pub tool: String,
    pub created_at: i64,
    pub latency_ms: Option<i64>,
    pub groundedness: Option<f64>,
}
