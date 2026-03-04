//! MCP resource definitions for proactive context.
//!
//! Resources allow HIEF to *push* context to agents rather than waiting
//! for them to query. Agents can subscribe to resources and receive
//! updates when the underlying data changes.
//!
//! # Resources
//!
//! | URI | Description |
//! |-----|-------------|
//! | `project://overview` | Index stats, active intents, project health |
//! | `project://conventions` | Machine-readable project rules |
//! | `project://health` | Latest eval scores, regressions, warnings |

use std::path::Path;

use serde::Serialize;

use crate::config::Config;
use crate::db::Database;
use crate::errors::Result;

/// Project overview resource content.
///
/// Provides a high-level summary of the project state that agents can
/// read at the start of every session.
#[derive(Debug, Clone, Serialize)]
pub struct ProjectOverview {
    /// Index statistics.
    pub index: IndexSummary,
    /// Active intents (in_progress).
    pub active_intents: Vec<IntentSummary>,
    /// Intents ready for work (approved + unblocked).
    pub ready_intents: Vec<IntentSummary>,
    /// Number of total intents by status.
    pub intent_counts: IntentCounts,
}

#[derive(Debug, Clone, Serialize)]
pub struct IndexSummary {
    pub total_files: usize,
    pub total_chunks: usize,
    pub languages: Vec<String>,
    pub last_indexed: Option<String>,
    pub has_vector_index: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct IntentSummary {
    pub id: String,
    pub title: String,
    pub kind: String,
    pub status: String,
    pub assigned_to: Option<String>,
    pub priority: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct IntentCounts {
    pub draft: usize,
    pub approved: usize,
    pub in_progress: usize,
    pub in_review: usize,
    pub done: usize,
    pub total: usize,
}

/// Project health resource content.
///
/// Provides the latest evaluation scores and any regressions.
#[derive(Debug, Clone, Serialize)]
pub struct ProjectHealth {
    /// Latest eval run results, one per golden set.
    pub eval_scores: Vec<EvalScoreSummary>,
    /// Whether any regressions were detected.
    pub has_regressions: bool,
    /// Warnings from doctor checks.
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EvalScoreSummary {
    pub golden_set: String,
    pub score: f64,
    pub passed: bool,
    pub git_commit: Option<String>,
    pub regression_status: String,
}

/// Conventions resource content.
///
/// Machine-readable project rules loaded from `.hief/conventions.toml`.
#[derive(Debug, Clone, Serialize)]
pub struct ProjectConventions {
    /// Whether conventions file exists.
    pub loaded: bool,
    /// Raw TOML content (for the agent to parse).
    pub content: Option<String>,
    /// Summary of conventions by severity.
    pub summary: ConventionSummary,
}

#[derive(Debug, Clone, Serialize)]
pub struct ConventionSummary {
    pub error_count: usize,
    pub warning_count: usize,
    pub info_count: usize,
}

/// Build the project overview resource.
pub async fn get_project_overview(db: &Database, project_root: &Path) -> Result<ProjectOverview> {
    // Get index stats
    let stats = crate::index::status(db, project_root).await?;

    let index_summary = IndexSummary {
        total_files: stats.total_files,
        total_chunks: stats.total_chunks,
        languages: stats.languages.keys().cloned().collect(),
        last_indexed: stats.last_indexed.map(|ts| {
            chrono::DateTime::from_timestamp(ts, 0)
                .map(|dt| dt.to_rfc3339())
                .unwrap_or_else(|| ts.to_string())
        }),
        has_vector_index: crate::index::vectors::vectors_dir(project_root).exists(),
    };

    // Get active intents
    let active = crate::graph::list_intents(db, Some("in_progress"), None).await?;
    let active_intents: Vec<IntentSummary> = active
        .iter()
        .map(|i| IntentSummary {
            id: i.id.clone(),
            title: i.title.clone(),
            kind: i.kind.clone(),
            status: i.status.clone(),
            assigned_to: i.assigned_to.clone(),
            priority: i.priority.clone(),
        })
        .collect();

    // Get ready intents
    let ready = crate::graph::ready_intents(db).await?;
    let ready_intents: Vec<IntentSummary> = ready
        .iter()
        .map(|i| IntentSummary {
            id: i.id.clone(),
            title: i.title.clone(),
            kind: i.kind.clone(),
            status: i.status.clone(),
            assigned_to: i.assigned_to.clone(),
            priority: i.priority.clone(),
        })
        .collect();

    // Count intents by status
    let all_intents = crate::graph::list_intents(db, None, None).await?;
    let intent_counts = IntentCounts {
        draft: all_intents.iter().filter(|i| i.status == "draft").count(),
        approved: all_intents
            .iter()
            .filter(|i| i.status == "approved")
            .count(),
        in_progress: all_intents
            .iter()
            .filter(|i| i.status == "in_progress")
            .count(),
        in_review: all_intents
            .iter()
            .filter(|i| i.status == "in_review")
            .count(),
        done: all_intents.iter().filter(|i| i.status == "done").count(),
        total: all_intents.len(),
    };

    Ok(ProjectOverview {
        index: index_summary,
        active_intents,
        ready_intents,
        intent_counts,
    })
}

/// Build the project conventions resource.
pub fn get_project_conventions(project_root: &Path) -> Result<ProjectConventions> {
    let conventions_path = project_root.join(".hief").join("conventions.toml");

    if !conventions_path.exists() {
        return Ok(ProjectConventions {
            loaded: false,
            content: None,
            summary: ConventionSummary {
                error_count: 0,
                warning_count: 0,
                info_count: 0,
            },
        });
    }

    let content = std::fs::read_to_string(&conventions_path)?;

    // Count conventions by severity
    let mut errors = 0usize;
    let mut warnings = 0usize;
    let mut infos = 0usize;

    // Simple TOML parsing for severity counts
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("severity") {
            if trimmed.contains("\"error\"") {
                errors += 1;
            } else if trimmed.contains("\"warning\"") {
                warnings += 1;
            } else if trimmed.contains("\"info\"") {
                infos += 1;
            }
        }
    }

    Ok(ProjectConventions {
        loaded: true,
        content: Some(content),
        summary: ConventionSummary {
            error_count: errors,
            warning_count: warnings,
            info_count: infos,
        },
    })
}

/// Build the project health resource.
pub async fn get_project_health(
    db: &Database,
    project_root: &Path,
    config: &Config,
) -> Result<ProjectHealth> {
    let mut eval_scores = Vec::new();
    let mut has_regressions = false;

    // Try to load golden sets and get latest scores
    let golden_dir = project_root.join(&config.eval.golden_set_path);
    if golden_dir.exists() {
        if let Ok(golden_sets) = crate::eval::golden::load_golden_sets(&golden_dir, None) {
            for gs in &golden_sets {
                // Get latest score history
                if let Ok(history) =
                    crate::eval::history::get_history(db, &gs.metadata.name, 1).await
                {
                    if let Some(latest) = history.first() {
                        let regression = crate::eval::history::check_regression(
                            db,
                            &gs.metadata.name,
                            latest.score,
                            5,
                        )
                        .await
                        .unwrap_or(crate::eval::RegressionStatus::NoHistory);

                        let regression_str = match &regression {
                            crate::eval::RegressionStatus::Regression { .. } => {
                                has_regressions = true;
                                "regression"
                            }
                            crate::eval::RegressionStatus::Warning { .. } => "warning",
                            crate::eval::RegressionStatus::Ok => "ok",
                            crate::eval::RegressionStatus::NoHistory => "no_history",
                        };

                        eval_scores.push(EvalScoreSummary {
                            golden_set: gs.metadata.name.clone(),
                            score: latest.score,
                            passed: latest.passed,
                            git_commit: latest.git_commit.clone(),
                            regression_status: regression_str.to_string(),
                        });
                    }
                }
            }
        }
    }

    // Collect warnings
    let mut warnings = Vec::new();

    // Check index freshness
    let stats = crate::index::status(db, project_root).await?;
    if stats.total_files == 0 {
        warnings.push("Index is empty — run `hief index build`".to_string());
    }

    // Check for conventions file
    let conventions_path = project_root.join(".hief").join("conventions.toml");
    if !conventions_path.exists() {
        warnings.push("No conventions.toml found — consider creating .hief/conventions.toml".to_string());
    }

    // Check for golden sets
    if !golden_dir.exists() || golden_dir.read_dir().map_or(true, |mut d| d.next().is_none()) {
        warnings.push("No golden sets found — evaluation cannot run".to_string());
    }

    Ok(ProjectHealth {
        eval_scores,
        has_regressions,
        warnings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_conventions_missing_file() {
        let tmp = tempfile::tempdir().unwrap();
        let result = get_project_conventions(tmp.path()).unwrap();
        assert!(!result.loaded);
        assert!(result.content.is_none());
    }

    #[test]
    fn test_conventions_with_file() {
        let tmp = tempfile::tempdir().unwrap();
        let hief_dir = tmp.path().join(".hief");
        std::fs::create_dir_all(&hief_dir).unwrap();
        std::fs::write(
            hief_dir.join("conventions.toml"),
            r#"
[rule1]
severity = "error"

[rule2]
severity = "warning"

[rule3]
severity = "info"
"#,
        )
        .unwrap();

        let result = get_project_conventions(tmp.path()).unwrap();
        assert!(result.loaded);
        assert!(result.content.is_some());
        assert_eq!(result.summary.error_count, 1);
        assert_eq!(result.summary.warning_count, 1);
        assert_eq!(result.summary.info_count, 1);
    }

    #[test]
    fn test_convention_summary_default() {
        let summary = ConventionSummary {
            error_count: 0,
            warning_count: 0,
            info_count: 0,
        };
        assert_eq!(summary.error_count, 0);
    }
}
