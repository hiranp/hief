//! `hief watch` — real filesystem watcher for implicit behavior tracking.

use std::path::Path;

use crate::config::Config;
use crate::errors::Result;

pub fn run_watch(
    project_root: &Path,
    config: &Config,
    agent: Option<&str>,
    debounce_ms: Option<u64>,
    conflict_window_secs: Option<u64>,
    json: bool,
) -> Result<()> {
    let agent_id = agent
        .map(ToOwned::to_owned)
        .or_else(|| std::env::var("HIEF_AGENT_ID").ok())
        .unwrap_or_else(|| "local-agent".to_string());

    let debounce = debounce_ms.unwrap_or(config.watcher.debounce_ms);
    let conflict_window = conflict_window_secs.unwrap_or(config.watcher.conflict_window_secs);

    crate::watcher::run(project_root, &agent_id, debounce, conflict_window, json)
}
