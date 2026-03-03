//! `hief eval` — evaluation and golden-set commands.

use std::path::Path;

use crate::config::Config;
use crate::db::Database;
use crate::errors::Result;

/// Run evaluation.
pub async fn eval_run(
    db: &Database,
    project_root: &Path,
    config: &Config,
    golden: Option<&str>,
    ci: bool,
    json: bool,
) -> Result<i32> {
    if ci {
        let exit_code = crate::eval::run_ci(db, project_root, &config.eval, golden).await?;
        return Ok(exit_code);
    }

    let results = crate::eval::run(db, project_root, &config.eval, golden).await?;

    if json {
        println!("{}", serde_json::to_string_pretty(&results).unwrap());
    } else {
        for result in &results {
            let icon = if result.passed { "✅" } else { "❌" };
            println!(
                "{} Golden set '{}': score {:.2} ({})",
                icon,
                result.golden_set,
                result.overall_score,
                if result.passed { "PASS" } else { "FAIL" },
            );
            for case in &result.cases {
                let case_icon = if case.passed { "✓" } else { "✗" };
                println!(
                    "  {} [{}] {} — {:.2}",
                    case_icon, case.priority, case.case_name, case.score,
                );
                for v in &case.violations {
                    println!("    ⚠ {}: '{}' in {}", v.kind, v.pattern, v.file);
                }
            }
        }
    }

    Ok(0)
}

/// Show evaluation report.
pub async fn eval_report(db: &Database, golden: &str, limit: usize, json: bool) -> Result<()> {
    let history = crate::eval::history::get_history(db, golden, limit).await?;

    if json {
        println!("{}", serde_json::to_string_pretty(&history).unwrap());
    } else if history.is_empty() {
        println!("No evaluation history for '{}'", golden);
    } else {
        println!("📈 Score history for '{}' (last {}):\n", golden, limit);
        for entry in &history {
            let icon = if entry.passed { "✅" } else { "❌" };
            let commit = entry
                .git_commit
                .as_deref()
                .map(|c| &c[..7.min(c.len())])
                .unwrap_or("N/A");
            println!(
                "  {} {:.2} — commit {} (ts: {})",
                icon, entry.score, commit, entry.created_at,
            );
        }
    }

    Ok(())
}

/// List golden sets.
pub fn eval_golden_list(project_root: &Path, config: &Config, json: bool) -> Result<()> {
    let sets = crate::eval::list_golden_sets(project_root, &config.eval)?;

    if json {
        println!("{}", serde_json::to_string_pretty(&sets).unwrap());
    } else if sets.is_empty() {
        println!("No golden sets found in {}", config.eval.golden_set_path);
    } else {
        println!("📝 Golden sets:");
        for name in &sets {
            println!("  - {}", name);
        }
    }

    Ok(())
}
