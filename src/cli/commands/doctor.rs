//! `hief doctor` — health-check command.

use std::path::Path;

use serde::Serialize;

use crate::config::Config;
use crate::db::Database;
use crate::errors::Result;
use crate::graph;

use super::hooks::hooks_install;

/// Result of a doctor health check.
#[derive(Debug, Clone, Serialize)]
pub struct DoctorReport {
    pub healthy: bool,
    pub checks: Vec<DoctorCheck>,
    pub fixes_applied: usize,
}

/// A single health check result.
#[derive(Debug, Clone, Serialize)]
pub struct DoctorCheck {
    pub name: String,
    pub status: String, // "ok", "warning", "error"
    pub message: String,
    pub fixable: bool,
    pub fixed: bool,
}

/// Run health checks on index, graph, and eval subsystems.
pub async fn doctor(
    db: &Database,
    project_root: &Path,
    config_path: &Path,
    config: &Config,
    fix: bool,
    json: bool,
) -> Result<()> {
    let mut checks = Vec::new();
    let mut fixes_applied = 0;

    // 0. Check config/binary version drift (upgrade hygiene)
    let binary_version = env!("CARGO_PKG_VERSION");
    if config.hief.version != binary_version {
        let mut check = DoctorCheck {
            name: "config_version".to_string(),
            status: "warning".to_string(),
            message: format!(
                "hief.toml version '{}' differs from binary '{}'",
                config.hief.version, binary_version
            ),
            fixable: true,
            fixed: false,
        };

        if fix {
            if upsert_hief_version_file(config_path, binary_version).is_ok() {
                check.status = "ok".to_string();
                check.message = format!("Updated hief.toml version to '{}'", binary_version);
                check.fixed = true;
                fixes_applied += 1;
            }
        }

        checks.push(check);
    } else {
        checks.push(DoctorCheck {
            name: "config_version".to_string(),
            status: "ok".to_string(),
            message: format!("Config version matches binary ({})", binary_version),
            fixable: false,
            fixed: false,
        });
    }

    // 1. Check if .hief directory exists
    let hief_dir = Config::hief_dir(project_root);
    if !hief_dir.exists() {
        checks.push(DoctorCheck {
            name: "hief_init".to_string(),
            status: "error".to_string(),
            message: "HIEF not initialized — run `hief init`".to_string(),
            fixable: false,
            fixed: false,
        });
    } else {
        checks.push(DoctorCheck {
            name: "hief_init".to_string(),
            status: "ok".to_string(),
            message: ".hief directory exists".to_string(),
            fixable: false,
            fixed: false,
        });
    }

    // 2. Check index staleness
    let stats = crate::index::status(db, project_root).await?;
    if stats.total_files == 0 {
        let mut check = DoctorCheck {
            name: "index_populated".to_string(),
            status: "warning".to_string(),
            message: "Index is empty — run `hief index build`".to_string(),
            fixable: true,
            fixed: false,
        };
        if fix {
            crate::index::build(db, project_root, &config.index).await?;
            check.status = "ok".to_string();
            check.message = "Index rebuilt successfully".to_string();
            check.fixed = true;
            fixes_applied += 1;
        }
        checks.push(check);
    } else {
        checks.push(DoctorCheck {
            name: "index_populated".to_string(),
            status: "ok".to_string(),
            message: format!(
                "{} files, {} chunks indexed",
                stats.total_files, stats.total_chunks
            ),
            fixable: false,
            fixed: false,
        });
    }

    // 3. Check graph integrity (cycles)
    let validation = graph::validate_graph(db).await?;
    if validation.has_cycles {
        checks.push(DoctorCheck {
            name: "graph_cycles".to_string(),
            status: "error".to_string(),
            message: format!(
                "Dependency cycles detected in {} nodes: {}",
                validation.cycle_nodes.len(),
                validation.cycle_nodes.join(", ")
            ),
            fixable: false,
            fixed: false,
        });
    } else {
        checks.push(DoctorCheck {
            name: "graph_cycles".to_string(),
            status: "ok".to_string(),
            message: "No dependency cycles".to_string(),
            fixable: false,
            fixed: false,
        });
    }

    // 4. Check for auto-blocked intents (blocked status from rejected deps)
    if validation.auto_blocked > 0 {
        checks.push(DoctorCheck {
            name: "graph_auto_blocked".to_string(),
            status: "warning".to_string(),
            message: format!(
                "{} intents auto-blocked due to rejected dependencies",
                validation.auto_blocked
            ),
            fixable: false,
            fixed: false,
        });
    }

    // 5. Check for stale intents (in_progress for too long)
    let all_intents = graph::list_intents(db, None, None).await?;
    let stale_intents: Vec<_> = all_intents
        .iter()
        .filter(|i| {
            i.status == "in_progress" && {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs() as i64;
                // Stale if in_progress for more than 7 days
                now - i.updated_at > 7 * 24 * 3600
            }
        })
        .collect();

    if !stale_intents.is_empty() {
        checks.push(DoctorCheck {
            name: "stale_intents".to_string(),
            status: "warning".to_string(),
            message: format!(
                "{} intents stuck in 'in_progress' for >7 days: {}",
                stale_intents.len(),
                stale_intents
                    .iter()
                    .map(|i| i.id.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            fixable: false,
            fixed: false,
        });
    } else {
        checks.push(DoctorCheck {
            name: "stale_intents".to_string(),
            status: "ok".to_string(),
            message: "No stale intents".to_string(),
            fixable: false,
            fixed: false,
        });
    }

    // 6. Check for orphaned intents (no edges and still in draft)
    let draft_intents: Vec<_> = all_intents
        .iter()
        .filter(|i| i.status == "draft")
        .collect();
    if draft_intents.len() > 10 {
        checks.push(DoctorCheck {
            name: "orphan_drafts".to_string(),
            status: "warning".to_string(),
            message: format!(
                "{} intents still in draft — consider reviewing or removing",
                draft_intents.len()
            ),
            fixable: false,
            fixed: false,
        });
    } else {
        checks.push(DoctorCheck {
            name: "orphan_drafts".to_string(),
            status: "ok".to_string(),
            message: format!("{} draft intents", draft_intents.len()),
            fixable: false,
            fixed: false,
        });
    }

    // 7. Check golden sets exist
    let golden_sets = crate::eval::list_golden_sets(project_root, &config.eval)?;
    if golden_sets.is_empty() {
        checks.push(DoctorCheck {
            name: "golden_sets".to_string(),
            status: "warning".to_string(),
            message: format!(
                "No golden sets found in {} — evaluation won't work",
                config.eval.golden_set_path
            ),
            fixable: false,
            fixed: false,
        });
    } else {
        checks.push(DoctorCheck {
            name: "golden_sets".to_string(),
            status: "ok".to_string(),
            message: format!("{} golden sets available", golden_sets.len()),
            fixable: false,
            fixed: false,
        });
    }

    // 8. Check git hooks
    let hooks_dir = project_root.join(".git/hooks");
    let post_commit_hook = hooks_dir.join("post-commit");
    if !post_commit_hook.exists()
        || !std::fs::read_to_string(&post_commit_hook)
            .unwrap_or_default()
            .contains("hief")
    {
        let mut check = DoctorCheck {
            name: "git_hooks".to_string(),
            status: "warning".to_string(),
            message: "HIEF git hooks not installed — run `hief hooks install`".to_string(),
            fixable: true,
            fixed: false,
        };
        if fix {
            if let Ok(()) = hooks_install(project_root, false) {
                check.status = "ok".to_string();
                check.message = "Git hooks installed".to_string();
                check.fixed = true;
                fixes_applied += 1;
            }
        }
        checks.push(check);
    } else {
        checks.push(DoctorCheck {
            name: "git_hooks".to_string(),
            status: "ok".to_string(),
            message: "Git hooks installed".to_string(),
            fixable: false,
            fixed: false,
        });
    }

    let healthy = checks.iter().all(|c| c.status != "error");

    let report = DoctorReport {
        healthy,
        checks,
        fixes_applied,
    };

    if json {
        println!("{}", serde_json::to_string_pretty(&report).unwrap());
    } else {
        println!("🩺 HIEF Doctor Report\n");
        for check in &report.checks {
            let icon = match check.status.as_str() {
                "ok" => "✅",
                "warning" => "⚠️ ",
                "error" => "❌",
                _ => "❓",
            };
            let fixed_tag = if check.fixed { " (FIXED)" } else { "" };
            println!(
                "  {} {} — {}{}",
                icon, check.name, check.message, fixed_tag
            );
        }
        println!();
        if report.healthy {
            println!("✅ Overall: healthy");
        } else {
            println!("❌ Overall: issues detected");
        }
        if report.fixes_applied > 0 {
            println!("🔧 {} fixes applied", report.fixes_applied);
        }
    }

    Ok(())
}

fn upsert_hief_version_file(path: &Path, version: &str) -> Result<()> {
    let existing = if path.exists() {
        std::fs::read_to_string(path)?
    } else {
        String::new()
    };

    let updated = upsert_hief_version_content(&existing, version);
    std::fs::write(path, updated)?;
    Ok(())
}

fn upsert_hief_version_content(content: &str, version: &str) -> String {
    let mut lines: Vec<String> = content.lines().map(|l| l.to_string()).collect();

    let mut hief_start: Option<usize> = None;
    let mut hief_end = lines.len();

    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if trimmed == "[hief]" {
            hief_start = Some(i);
            continue;
        }

        if hief_start.is_some() && trimmed.starts_with('[') && trimmed.ends_with(']') {
            hief_end = i;
            break;
        }
    }

    match hief_start {
        Some(start) => {
            let mut replaced = false;
            for line in lines.iter_mut().take(hief_end).skip(start + 1) {
                let trimmed = line.trim_start();
                if trimmed.starts_with("version") {
                    *line = format!("version = \"{}\"", version);
                    replaced = true;
                    break;
                }
            }

            if !replaced {
                lines.insert(start + 1, format!("version = \"{}\"", version));
            }
        }
        None => {
            if !lines.is_empty() && !lines.last().map(|l| l.is_empty()).unwrap_or(false) {
                lines.push(String::new());
            }
            lines.push("[hief]".to_string());
            lines.push(format!("version = \"{}\"", version));
        }
    }

    let mut output = lines.join("\n");
    if !output.ends_with('\n') {
        output.push('\n');
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_upsert_hief_version_replaces_existing() {
        let input = "[hief]\nversion = \"0.1.0\"\n\n[index]\nchunk_strategy = \"ast\"\n";
        let output = upsert_hief_version_content(input, "0.2.0");
        assert!(output.contains("[hief]\nversion = \"0.2.0\""));
    }

    #[test]
    fn test_upsert_hief_version_inserts_when_missing() {
        let input = "[index]\nchunk_strategy = \"ast\"\n";
        let output = upsert_hief_version_content(input, "0.2.0");
        assert!(output.contains("[hief]\nversion = \"0.2.0\""));
    }
}
