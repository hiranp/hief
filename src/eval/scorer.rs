//! Scoring engine: pattern matching against code index.

use serde::Serialize;
use std::path::Path;
use tracing::debug;

use crate::db::Database;
use crate::errors::{HiefError, Result};
use crate::eval::golden::{EvalCase, GoldenSet};

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

// Helper: search for literal substring occurrences using instr().
async fn search_literal(
    db: &Database,
    substr: &str,
    file_glob: Option<&String>,
    limit: usize,
) -> Result<Vec<(String, u32)>> {
    // returns vector of (file_path, start_line)
    let conn = db.conn();
    let mut sql = String::from(
        "SELECT c.file_path, c.start_line FROM chunks c WHERE instr(c.content, ?1) > 0",
    );
    let mut params: Vec<String> = vec![substr.to_string()];
    if let Some(glob) = file_glob {
        sql.push_str(" AND c.file_path GLOB ?2");
        params.push(glob.clone());
    }
    sql.push_str(&format!(" LIMIT {}", limit));
    let mut rows = if params.len() == 1 {
        conn.query(&sql, [params[0].as_str()]).await?
    } else {
        conn.query(&sql, [params[0].as_str(), params[1].as_str()])
            .await?
    };
    let mut res = Vec::new();
    while let Some(row) = rows.next().await? {
        let file_path: String = row.get(0)?;
        let start_line: i64 = row.get(1)?;
        res.push((file_path, start_line as u32));
    }
    Ok(res)
}

/// Evaluate a single case against the code index.
async fn evaluate_case(db: &Database, case: &EvalCase) -> Result<CaseResult> {
    let mut violations = Vec::new();
    let total_checks = case.checks.must_contain.len() + case.checks.must_not_contain.len();

    // Check must_contain patterns
    for pattern in &case.checks.must_contain {
        let results = search_literal(
            db,
            pattern,
            case.checks.file_patterns.as_ref().and_then(|v| v.first()),
            1,
        )
        .await?;

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
        let results = search_literal(
            db,
            pattern,
            case.checks.file_patterns.as_ref().and_then(|v| v.first()),
            5,
        )
        .await?;

        for (file, line) in &results {
            violations.push(Violation {
                kind: "must_not_contain_found".to_string(),
                pattern: pattern.clone(),
                file: file.clone(),
                line: Some(*line),
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
// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;

    #[tokio::test]
    async fn test_search_literal_punctuation() {
        let db = Database::open_memory().await.unwrap();
        // create chunks table entry
        db.conn()
            .execute(
                "INSERT INTO chunks (file_path, language, content, start_line, end_line, content_hash)
                 VALUES ('foo.rs', 'rust', 'let x = v.unwrap();', 0, 0, 'h');",
                (),
            )
            .await
            .unwrap();

        let results = search_literal(
            &db,
            ".unwrap()",
            None,
            10,
        )
        .await
        .unwrap();
        assert!(!results.is_empty(), "literal search should find dot pattern");
    }

    #[tokio::test]
    async fn test_evaluate_case_handles_dot_pattern() {
        let db = Database::open_memory().await.unwrap();
        // insert a chunk containing the forbidden pattern
        db.conn()
            .execute(
                "INSERT INTO chunks (file_path, language, content, start_line, end_line, content_hash)
                 VALUES ('foo.rs', 'rust', 'let x = v.unwrap();', 0, 0, 'h');",
                (),
            )
            .await
            .unwrap();

        let case = EvalCase {
            id: "t1".to_string(),
            name: "dot test".to_string(),
            priority: "high".to_string(),
            intent: None,
            checks: crate::eval::golden::EvalChecks {
                must_contain: Vec::new(),
                must_not_contain: vec![".unwrap()".to_string()],
                file_patterns: None,
            },
        };

        let result = evaluate_case(&db, &case).await.unwrap();
        assert!(!result.violations.is_empty());
    }
}