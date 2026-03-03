//! Scoring engine: pattern matching against code index.

use serde::Serialize;
use std::path::Path;
use tracing::debug;

use crate::db::Database;
use crate::errors::{HiefError, Result};
use crate::eval::golden::{EvalCase, GoldenSet};
use crate::index::search::{SearchQuery, self};

/// Overall result of evaluating a golden set.
#[derive(Debug, Clone, Serialize)]
pub struct EvalResult {
    pub golden_set: String,
    pub overall_score: f64,
    pub passed: bool,
    pub cases: Vec<CaseResult>,
    pub git_commit: Option<String>,
}

/// Result of evaluating a single case.
#[derive(Debug, Clone, Serialize)]
pub struct CaseResult {
    pub case_id: String,
    pub case_name: String,
    pub priority: String,
    pub passed: bool,
    pub score: f64,
    pub violations: Vec<Violation>,
}

/// A specific violation found during evaluation.
#[derive(Debug, Clone, Serialize)]
pub struct Violation {
    pub kind: String,
    pub pattern: String,
    pub file: String,
    pub line: Option<u32>,
}

/// Evaluate a golden set against the code index.
pub async fn evaluate(
    db: &Database,
    golden_set: &GoldenSet,
    _project_root: &Path,
) -> Result<EvalResult> {
    let mut case_results = Vec::new();
    let git_commit = get_git_commit().await.ok();

    for case in &golden_set.cases {
        let case_result = evaluate_case(db, case).await?;
        case_results.push(case_result);
    }

    // Calculate overall score with priority weighting
    let (total_weight, weighted_score) =
        case_results
            .iter()
            .fold((0.0f64, 0.0f64), |(tw, ws), c| {
                let w = priority_weight(&c.priority);
                (tw + w, ws + c.score * w)
            });

    let overall_score = if total_weight > 0.0 {
        weighted_score / total_weight
    } else {
        1.0
    };

    // Overall passes if no critical failures and score meets threshold
    let all_critical_pass = case_results
        .iter()
        .filter(|c| c.priority == "critical")
        .all(|c| c.passed);

    let passed = all_critical_pass && overall_score >= 0.85;

    Ok(EvalResult {
        golden_set: golden_set.metadata.name.clone(),
        overall_score,
        passed,
        cases: case_results,
        git_commit,
    })
}

/// Evaluate a single case against the code index.
async fn evaluate_case(db: &Database, case: &EvalCase) -> Result<CaseResult> {
    let mut violations = Vec::new();
    let total_checks = case.checks.must_contain.len() + case.checks.must_not_contain.len();

    // Check must_contain patterns
    for pattern in &case.checks.must_contain {
        let mut query = SearchQuery::new(pattern.as_str());
        query.top_k = 1;

        // Apply file pattern filter if specified
        if let Some(patterns) = &case.checks.file_patterns {
            if let Some(first_pattern) = patterns.first() {
                query.file_pattern = Some(first_pattern.clone());
            }
        }

        let results = search::search(db, &query).await?;

        if results.is_empty() {
            violations.push(Violation {
                kind: "must_contain_missing".to_string(),
                pattern: pattern.clone(),
                file: "N/A".to_string(),
                line: None,
            });
        }
    }

    // Check must_not_contain patterns
    for pattern in &case.checks.must_not_contain {
        let mut query = SearchQuery::new(pattern.as_str());
        query.top_k = 5;

        if let Some(patterns) = &case.checks.file_patterns {
            if let Some(first_pattern) = patterns.first() {
                query.file_pattern = Some(first_pattern.clone());
            }
        }

        let results = search::search(db, &query).await?;

        for result in &results {
            violations.push(Violation {
                kind: "must_not_contain_found".to_string(),
                pattern: pattern.clone(),
                file: result.file_path.clone(),
                line: Some(result.start_line),
            });
        }
    }

    let score = if total_checks > 0 {
        1.0 - (violations.len() as f64 / total_checks as f64)
    } else {
        1.0
    };

    let score = score.max(0.0);
    let passed = violations.is_empty();

    debug!(
        "Case '{}': score={:.2}, violations={}, passed={}",
        case.name,
        score,
        violations.len(),
        passed
    );

    Ok(CaseResult {
        case_id: case.id.clone(),
        case_name: case.name.clone(),
        priority: case.priority.clone(),
        passed,
        score,
        violations,
    })
}

/// Get priority weight for scoring.
fn priority_weight(priority: &str) -> f64 {
    match priority {
        "critical" => 2.0,
        "high" => 1.5,
        "medium" => 1.0,
        "low" => 0.5,
        _ => 1.0,
    }
}

/// Get current git commit hash.
async fn get_git_commit() -> Result<String> {
    let output = tokio::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .await?;

    if !output.status.success() {
        return Err(HiefError::Other("not a git repository".to_string()));
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}
