//! Filesystem watcher service for implicit behavior tracking and conflict warnings.

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use serde::Serialize;

use crate::errors::Result;

#[derive(Debug, Clone, Serialize)]
pub struct ActivityEvent {
    pub timestamp: i64,
    pub agent: String,
    pub path: String,
    pub kind: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ConflictWarning {
    pub path: String,
    pub current_agent: String,
    pub other_agent: String,
    pub age_seconds: i64,
}

pub fn run(
    project_root: &Path,
    agent: &str,
    debounce_ms: u64,
    conflict_window_secs: u64,
    json: bool,
) -> Result<()> {
    let activity_dir = project_root.join(".hief").join("activity");
    std::fs::create_dir_all(&activity_dir)?;
    let log_path = activity_dir.join("events.jsonl");

    let (tx, rx) = mpsc::channel::<notify::Result<Event>>();

    let mut watcher: RecommendedWatcher = notify::recommended_watcher(move |res| {
        let _ = tx.send(res);
    })
    .map_err(|e| crate::errors::HiefError::Other(format!("watcher init failed: {}", e)))?;

    watcher
        .watch(project_root, RecursiveMode::Recursive)
        .map_err(|e| crate::errors::HiefError::Other(format!("watch setup failed: {}", e)))?;

    if json {
        println!(
            "{}",
            serde_json::json!({
                "status": "watching",
                "root": project_root.display().to_string(),
                "agent": agent,
                "debounce_ms": debounce_ms,
                "conflict_window_secs": conflict_window_secs,
            })
        );
    } else {
        println!(
            "Watching {} as agent '{}' (debounce={}ms, conflict_window={}s)",
            project_root.display(),
            agent,
            debounce_ms,
            conflict_window_secs
        );
    }

    let mut recent_by_path: HashMap<String, i64> = HashMap::new();
    let mut recent_events: VecDeque<ActivityEvent> = VecDeque::new();

    loop {
        let event = match rx.recv() {
            Ok(Ok(e)) => e,
            Ok(Err(e)) => {
                if json {
                    println!("{}", serde_json::json!({"watch_error": e.to_string()}));
                } else {
                    eprintln!("watch error: {}", e);
                }
                continue;
            }
            Err(_) => break,
        };

        if !is_core_event(&event.kind) {
            continue;
        }

        for path in &event.paths {
            if should_ignore(project_root, path) {
                continue;
            }

            let Some(rel) = to_relative(project_root, path) else {
                continue;
            };

            let now = now_ts();
            if let Some(last_seen) = recent_by_path.get(&rel)
                && now - *last_seen < (debounce_ms as i64 / 1000).max(1)
            {
                continue;
            }
            recent_by_path.insert(rel.clone(), now);

            let activity = ActivityEvent {
                timestamp: now,
                agent: agent.to_string(),
                path: rel.clone(),
                kind: event_kind_label(&event.kind),
            };

            append_event(&log_path, &activity)?;

            if let Some(conflict) = detect_conflict(&recent_events, &activity, conflict_window_secs)
            {
                if json {
                    println!(
                        "{}",
                        serde_json::json!({"conflict": conflict, "event": activity})
                    );
                } else {
                    eprintln!(
                        "CONFLICT: '{}' touched by '{}' {}s ago and now by '{}'",
                        conflict.path,
                        conflict.other_agent,
                        conflict.age_seconds,
                        conflict.current_agent,
                    );
                }
            } else if json {
                println!("{}", serde_json::json!({"event": activity}));
            } else {
                println!("{} [{}]", activity.path, activity.kind);
            }

            recent_events.push_back(activity);
            while let Some(front) = recent_events.front() {
                if now - front.timestamp > conflict_window_secs as i64 {
                    let _ = recent_events.pop_front();
                } else {
                    break;
                }
            }
        }
    }

    Ok(())
}

fn is_core_event(kind: &EventKind) -> bool {
    kind.is_create() || kind.is_modify() || kind.is_remove()
}

fn event_kind_label(kind: &EventKind) -> String {
    if kind.is_create() {
        "create".to_string()
    } else if kind.is_modify() {
        "modify".to_string()
    } else if kind.is_remove() {
        "remove".to_string()
    } else {
        format!("{:?}", kind)
    }
}

fn should_ignore(project_root: &Path, path: &Path) -> bool {
    if let Some(rel) = to_relative(project_root, path) {
        rel.starts_with(".git/")
            || rel.starts_with("target/")
            || rel.starts_with("node_modules/")
            || rel.starts_with(".hief/activity/")
            || rel.ends_with(".swp")
            || rel.ends_with(".tmp")
    } else {
        true
    }
}

fn to_relative(project_root: &Path, path: &Path) -> Option<String> {
    let absolute: PathBuf = if path.is_absolute() {
        path.to_path_buf()
    } else {
        project_root.join(path)
    };

    absolute
        .strip_prefix(project_root)
        .ok()
        .map(|p| p.to_string_lossy().replace('\\', "/"))
}

fn append_event(path: &Path, event: &ActivityEvent) -> Result<()> {
    use std::io::Write;
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    let line = serde_json::to_string(event)
        .map_err(|e| crate::errors::HiefError::Other(format!("event encode failed: {}", e)))?;
    writeln!(f, "{}", line)?;
    Ok(())
}

fn detect_conflict(
    history: &VecDeque<ActivityEvent>,
    current: &ActivityEvent,
    window_secs: u64,
) -> Option<ConflictWarning> {
    for ev in history.iter().rev() {
        if ev.path != current.path {
            continue;
        }
        if ev.agent == current.agent {
            continue;
        }
        let age = current.timestamp - ev.timestamp;
        if age <= window_secs as i64 {
            return Some(ConflictWarning {
                path: current.path.clone(),
                current_agent: current.agent.clone(),
                other_agent: ev.agent.clone(),
                age_seconds: age,
            });
        }
    }
    None
}

fn now_ts() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::from_secs(0))
        .as_secs() as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conflict_detects_cross_agent_touch() {
        let mut history = VecDeque::new();
        history.push_back(ActivityEvent {
            timestamp: 100,
            agent: "agent-a".to_string(),
            path: "src/main.rs".to_string(),
            kind: "modify".to_string(),
        });
        let current = ActivityEvent {
            timestamp: 120,
            agent: "agent-b".to_string(),
            path: "src/main.rs".to_string(),
            kind: "modify".to_string(),
        };
        let c = detect_conflict(&history, &current, 60).expect("expected conflict");
        assert_eq!(c.other_agent, "agent-a");
    }

    #[test]
    fn conflict_ignores_same_agent() {
        let mut history = VecDeque::new();
        history.push_back(ActivityEvent {
            timestamp: 100,
            agent: "agent-a".to_string(),
            path: "src/main.rs".to_string(),
            kind: "modify".to_string(),
        });
        let current = ActivityEvent {
            timestamp: 120,
            agent: "agent-a".to_string(),
            path: "src/main.rs".to_string(),
            kind: "modify".to_string(),
        };
        assert!(detect_conflict(&history, &current, 60).is_none());
    }
}
