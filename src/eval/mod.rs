//! Evaluation engine: golden sets, pattern matching, scoring, and history.

pub mod golden;
pub mod history;
pub mod scorer;

use serde::Serialize;
use std::path::Path;

use crate::config::EvalConfig;
use crate::db::Database;
use crate::errors::Result;

pub use self::scorer::EvalResult;

/// Run evaluation against a specific golden set (or all).
pub async fn run(
    db: &Database,
    project_root: &Path,
    config: &EvalConfig,
    golden_set_name: Option<&str>,
) -> Result<Vec<EvalResult>> {
    let golden_dir = project_root.join(&config.golden_set_path);
    let sets = golden::load_golden_sets(&golden_dir, golden_set_name)?;

    let mut results = Vec::new();

    for set in &sets {
        let result = scorer::evaluate(db, set, project_root, config.min_score).await?;
        history::store_result(db, &result).await?;
        results.push(result);
    }

    Ok(results)
}

/// Run in CI mode: returns exit code 0 (pass) or 1 (fail).
pub async fn run_ci(
    db: &Database,
    project_root: &Path,
    config: &EvalConfig,
    golden_set_name: Option<&str>,
) -> Result<i32> {
    let results = run(db, project_root, config, golden_set_name).await?;

    for result in &results {
        // Check threshold
        if result.overall_score < config.min_score {
            eprintln!(
                "❌ Golden set '{}': score {:.2} below threshold {:.2}",
                result.golden_set, result.overall_score, config.min_score
            );
            return Ok(1);
        }

        // Check critical case failures
        let critical_failures: Vec<_> = result
            .cases
            .iter()
            .filter(|c| c.priority == "critical" && !c.passed)
            .collect();

        if !critical_failures.is_empty() {
            for f in &critical_failures {
                eprintln!("❌ Critical case '{}' failed", f.case_name);
            }
            return Ok(1);
        }

        // Check regression
        if config.fail_on_regression {
            let regression =
                history::check_regression(db, &result.golden_set, result.overall_score, 5).await?;
            if let RegressionStatus::Regression { delta, .. } = regression {
                eprintln!(
                    "❌ Score regression: {:.2} below moving average",
                    delta.abs()
                );
                return Ok(1);
            }
        }
    }

    eprintln!("✅ All checks passed");
    Ok(0)
}

/// List available golden sets.
pub fn list_golden_sets(project_root: &Path, config: &EvalConfig) -> Result<Vec<String>> {
    let golden_dir = project_root.join(&config.golden_set_path);
    golden::list_sets(&golden_dir)
}

/// Regression status for score comparison.
#[derive(Debug, Clone, Serialize)]
pub enum RegressionStatus {
    NoHistory,
    Ok,
    Warning {
        current: f64,
        average: f64,
        delta: f64,
    },
    Regression {
        current: f64,
        average: f64,
        delta: f64,
    },
}
