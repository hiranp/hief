//! Score history storage and regression detection.

use uuid::Uuid;

use crate::db::Database;
use crate::errors::{HiefError, Result};
use crate::eval::RegressionStatus;
use crate::eval::scorer::EvalResult;

/// Store an evaluation result in the history.
pub async fn store_result(db: &Database, result: &EvalResult) -> Result<()> {
    let id = Uuid::new_v4().to_string();
    let details =
        serde_json::to_string(&result.cases).map_err(|e| HiefError::Other(e.to_string()))?;

    db.conn()
        .execute(
            "INSERT INTO eval_runs (id, golden_set, overall_score, passed, details, git_commit)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            libsql::params![
                id.as_str(),
                result.golden_set.as_str(),
                result.overall_score,
                result.passed,
                details.as_str(),
                result.git_commit.as_deref().unwrap_or(""),
            ],
        )
        .await
        .map_err(HiefError::Database)?;

    Ok(())
}

/// Get score history for a golden set.
pub async fn get_history(db: &Database, golden_set: &str, limit: usize) -> Result<Vec<ScoreEntry>> {
    let mut rows = db
        .conn()
        .query(
            "SELECT id, overall_score, passed, git_commit, created_at
             FROM eval_runs
             WHERE golden_set = ?1
             ORDER BY created_at DESC
             LIMIT ?2",
            libsql::params![golden_set, limit as i64],
        )
        .await
        .map_err(HiefError::Database)?;

    let mut entries = Vec::new();
    while let Some(row) = rows.next().await.map_err(HiefError::Database)? {
        entries.push(ScoreEntry {
            id: row.get(0).map_err(HiefError::Database)?,
            score: row.get(1).map_err(HiefError::Database)?,
            passed: row.get::<i64>(2).map_err(HiefError::Database)? != 0,
            git_commit: row.get::<String>(3).ok().filter(|s| !s.is_empty()),
            created_at: row.get(4).map_err(HiefError::Database)?,
        });
    }

    Ok(entries)
}

/// A single score entry from history.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ScoreEntry {
    pub id: String,
    pub score: f64,
    pub passed: bool,
    pub git_commit: Option<String>,
    pub created_at: i64,
}

/// Check for score regression against recent history.
pub async fn check_regression(
    db: &Database,
    golden_set: &str,
    current_score: f64,
    window: usize,
) -> Result<RegressionStatus> {
    let history = get_history(db, golden_set, window).await?;

    if history.is_empty() {
        return Ok(RegressionStatus::NoHistory);
    }

    let scores: Vec<f64> = history.iter().map(|e| e.score).collect();
    let avg: f64 = scores.iter().sum::<f64>() / scores.len() as f64;
    let delta = current_score - avg;

    if delta < -0.1 {
        Ok(RegressionStatus::Regression {
            current: current_score,
            average: avg,
            delta,
        })
    } else if delta < -0.05 {
        Ok(RegressionStatus::Warning {
            current: current_score,
            average: avg,
            delta,
        })
    } else {
        Ok(RegressionStatus::Ok)
    }
}
