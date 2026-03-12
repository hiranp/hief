//! Scoring engine: pattern matching + structural search against code index.
//!
//! Evaluates golden sets using three check modes:
//! 1. **Literal substring** — `must_contain` / `must_not_contain` via SQL `instr()`
//! 2. **Structural (ast-grep)** — `structural_must_contain` / `structural_must_not_contain`
//! 3. **Differential** — When `diff_only = true`, only checks files changed since last eval

use serde::Serialize;
use schemars::JsonSchema;
use std::path::Path;
use tracing::{debug, warn};

use crate::db::Database;
use crate::errors::{HiefError, Result};
use crate::eval::golden::{EvalCase, GoldenSet};

/// Overall result of evaluating a golden set.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct EvalResult {
    pub golden_set: String,
    pub overall_score: f64,
    pub passed: bool,
    pub cases: Vec<CaseResult>,
    pub git_commit: Option<String>,
    /// Files that were evaluated (all if diff_only was false, changed-only otherwise).
    pub scope: EvalScope,
}

/// Describes what was evaluated.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct EvalScope {
    /// "full" or "diff"
    pub mode: String,
    /// Number of files evaluated.
    pub files_evaluated: usize,
    /// Base commit for diff (only set when mode == "diff").
    pub base_commit: Option<String>,
}

/// Result of evaluating a single case.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct CaseResult {
    pub case_id: String,
    pub case_name: String,
    pub priority: String,
    pub passed: bool,
    pub score: f64,
    pub violations: Vec<Violation>,
}

/// A specific violation found during evaluation.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct Violation {
    pub kind: String,
    pub pattern: String,
    pub file: String,
    pub line: Option<u32>,
    /// Additional context around the violation.
    pub context: Option<String>,
}

/// Evaluate a golden set against the code index.
pub async fn evaluate(
    db: &Database,
    golden_set: &GoldenSet,
    project_root: &Path,
    min_score: f64,
) -> Result<EvalResult> {
    let mut case_results = Vec::new();
    let git_commit = get_git_commit().await.ok();

    // Determine diff scope if any case uses diff_only
    let any_diff = golden_set.cases.iter().any(|c| c.checks.diff_only);
    let changed_files = if any_diff {
        let last_commit = get_last_eval_commit(db, &golden_set.metadata.name).await;
        match &last_commit {
            Some(base) => get_changed_files(base).await.unwrap_or_default(),
            None => Vec::new(), // No history → evaluate all files
        }
    } else {
        Vec::new()
    };

    let base_commit_for_diff = if any_diff {
        get_last_eval_commit(db, &golden_set.metadata.name).await
    } else {
        None
    };

    let mut total_files_evaluated = 0usize;
    let mut eval_mode = "full".to_string();

    for case in &golden_set.cases {
        let diff_files = if case.checks.diff_only && !changed_files.is_empty() {
            eval_mode = "diff".to_string();
            Some(&changed_files)
        } else {
            None
        };
        let case_result = evaluate_case(db, case, project_root, diff_files).await?;
        total_files_evaluated += count_files_in_case(&case_result);
        case_results.push(case_result);
    }

    // Calculate overall score with priority weighting
    let (total_weight, weighted_score) =
        case_results.iter().fold((0.0f64, 0.0f64), |(tw, ws), c| {
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

    let passed = all_critical_pass && overall_score >= min_score;

    Ok(EvalResult {
        golden_set: golden_set.metadata.name.clone(),
        overall_score,
        passed,
        cases: case_results,
        git_commit,
        scope: EvalScope {
            mode: eval_mode,
            files_evaluated: total_files_evaluated,
            base_commit: base_commit_for_diff,
        },
    })
}

/// Parse a structural pattern entry like `"rust:$X.unwrap()"` into (language, pattern).
fn parse_structural_entry(entry: &str) -> Option<(&str, &str)> {
    let colon_pos = entry.find(':')?;
    let lang = entry[..colon_pos].trim();
    let pattern = entry[colon_pos + 1..].trim();
    if lang.is_empty() || pattern.is_empty() {
        return None;
    }
    Some((lang, pattern))
}

/// Search for literal substring occurrences using `instr()`.
async fn search_literal(
    db: &Database,
    substr: &str,
    file_globs: &[String],
    diff_files: Option<&[String]>,
    limit: usize,
) -> Result<Vec<(String, u32)>> {
    let conn = db.conn();

    // Build base query
    let mut sql = String::from(
        "SELECT c.file_path, c.start_line FROM chunks c WHERE instr(c.content, ?1) > 0",
    );
    let mut params: Vec<String> = vec![substr.to_string()];

    // Multiple file_globs are OR-combined: file must match ANY of them
    if !file_globs.is_empty() {
        let or_clauses: Vec<String> = file_globs
            .iter()
            .enumerate()
            .map(|(i, _)| format!("c.file_path GLOB ?{}", params.len() + i + 1))
            .collect();
        sql.push_str(&format!(" AND ({})", or_clauses.join(" OR ")));
        for g in file_globs {
            params.push(g.clone());
        }
    }

    // If diff_only, restrict to changed files
    if let Some(files) = diff_files {
        if !files.is_empty() {
            let placeholders: Vec<String> = files
                .iter()
                .enumerate()
                .map(|(i, _)| format!("?{}", params.len() + i + 1))
                .collect();
            sql.push_str(&format!(
                " AND c.file_path IN ({})",
                placeholders.join(", ")
            ));
            for f in files {
                params.push(f.clone());
            }
        }
    }

    sql.push_str(&format!(" LIMIT {}", limit));

    // Execute with dynamic params
    let mut rows = conn.query(&sql, params_to_libsql(&params)).await?;

    let mut res = Vec::new();
    while let Some(row) = rows.next().await? {
        let file_path: String = row.get(0)?;
        let start_line: i64 = row.get(1)?;
        res.push((file_path, start_line as u32));
    }
    Ok(res)
}

/// Convert a `Vec<String>` to libsql params.
fn params_to_libsql(params: &[String]) -> Vec<libsql::Value> {
    params
        .iter()
        .map(|s| libsql::Value::from(s.as_str()))
        .collect()
}

/// Run structural search for a given pattern/language, optionally restricted to diff files.
fn run_structural_check(
    project_root: &Path,
    language: &str,
    pattern: &str,
    file_globs: &[String],
    diff_files: Option<&[String]>,
    top_k: usize,
) -> Result<Vec<crate::index::structural::StructuralMatch>> {
    use crate::index::structural;

    let mut query = structural::StructuralQuery::new(pattern, language);
    query.top_k = top_k;

    let all_matches = structural::search(project_root, &query)?;

    // Apply file_globs OR filter + diff filter
    let filtered: Vec<_> = all_matches
        .into_iter()
        .filter(|m| {
            // Must match at least one glob (OR semantics); empty list = all files pass
            if !file_globs.is_empty() && !file_globs.iter().any(|g| glob_matches(&m.file_path, g)) {
                return false;
            }
            // Check diff filter
            if let Some(files) = diff_files {
                if !files.contains(&m.file_path) {
                    return false;
                }
            }
            true
        })
        .collect();

    Ok(filtered)
}

/// Simple glob matching (supports `*` and `**`).
fn glob_matches(path: &str, pattern: &str) -> bool {
    // Use a simple approach: convert glob to a check
    if pattern.contains("**") {
        // "src/**/*.rs" → check if path starts with prefix and ends with suffix
        let parts: Vec<&str> = pattern.splitn(2, "**").collect();
        let prefix = parts[0].trim_end_matches('/');
        let suffix = if parts.len() > 1 {
            parts[1].trim_start_matches('/')
        } else {
            ""
        };
        let prefix_ok = prefix.is_empty() || path.starts_with(prefix);
        let suffix_ok = if suffix.contains('*') {
            // "*.rs" → check extension
            let ext = suffix.trim_start_matches('*');
            path.ends_with(ext)
        } else {
            suffix.is_empty() || path.ends_with(suffix)
        };
        prefix_ok && suffix_ok
    } else if pattern.contains('*') {
        // "*.rs" → just check extension
        let ext = pattern.trim_start_matches('*');
        path.ends_with(ext)
    } else {
        path == pattern
    }
}

/// Evaluate a single case against the code index, with optional diff restriction.
async fn evaluate_case(
    db: &Database,
    case: &EvalCase,
    project_root: &Path,
    diff_files: Option<&Vec<String>>,
) -> Result<CaseResult> {
    let mut violations = Vec::new();
    let file_globs: &[String] = &case.checks.file_patterns;
    let exclude_globs: &[String] = &case.checks.exclude_file_patterns;
    let diff_slice = diff_files.map(|v| v.as_slice());

    // --- Literal checks ---

    let total_literal = case.checks.must_contain.len() + case.checks.must_not_contain.len();

    // Check must_contain patterns (literal)
    for pattern in &case.checks.must_contain {
        let results = search_literal(db, pattern, file_globs, diff_slice, 1).await?;

        // filter out excluded files
        let results: Vec<_> = results
            .into_iter()
            .filter(|(file, _)| {
                !exclude_globs.iter().any(|g| glob_matches(file, g))
            })
            .collect();
        if results.is_empty() {
            violations.push(Violation {
                kind: "must_contain_missing".to_string(),
                pattern: pattern.clone(),
                file: "N/A".to_string(),
                line: None,
                context: None,
            });
        }
    }

    // Check must_not_contain patterns (literal)
    for pattern in &case.checks.must_not_contain {
        let results = search_literal(db, pattern, file_globs, diff_slice, 5).await?;

        for (file, line) in &results {
            // ignore excluded files
            if exclude_globs.iter().any(|g| glob_matches(file, g)) {
                continue;
            }
            violations.push(Violation {
                kind: "must_not_contain_found".to_string(),
                pattern: pattern.clone(),
                file: file.clone(),
                line: Some(*line),
                context: None,
            });
        }
    }

    // --- Structural (ast-grep) checks ---

    let total_structural =
        case.checks.structural_must_contain.len() + case.checks.structural_must_not_contain.len();

    // Check structural_must_contain
    for entry in &case.checks.structural_must_contain {
        match parse_structural_entry(entry) {
            Some((lang, pattern)) => {
                match run_structural_check(project_root, lang, pattern, file_globs, diff_slice, 1) {
                    Ok(matches) => {
                        // drop any matches in excluded paths
                        let non_excluded: Vec<_> = matches
                            .into_iter()
                            .filter(|m| !exclude_globs.iter().any(|g| glob_matches(&m.file_path, g)))
                            .collect();
                        if non_excluded.is_empty() {
                            violations.push(Violation {
                                kind: "structural_must_contain_missing".to_string(),
                                pattern: entry.clone(),
                                file: "N/A".to_string(),
                                line: None,
                                context: Some(format!("No AST match for pattern '{}'", pattern)),
                            });
                        }
                    }
                    Err(e) => {
                        warn!("Structural search failed for pattern '{}': {}", pattern, e);
                        violations.push(Violation {
                            kind: "structural_check_error".to_string(),
                            pattern: entry.clone(),
                            file: "N/A".to_string(),
                            line: None,
                            context: Some(format!("Error: {}", e)),
                        });
                    }
                }
            }
            None => {
                violations.push(Violation {
                    kind: "invalid_structural_pattern".to_string(),
                    pattern: entry.clone(),
                    file: "N/A".to_string(),
                    line: None,
                    context: Some(
                        "Expected format 'language:pattern' (e.g. 'rust:$X.unwrap()')".to_string(),
                    ),
                });
            }
        }
    }

    // Check structural_must_not_contain
    for entry in &case.checks.structural_must_not_contain {
        match parse_structural_entry(entry) {
            Some((lang, pattern)) => {
                match run_structural_check(project_root, lang, pattern, file_globs, diff_slice, 10) {
                    Ok(matches) => {
                        for m in &matches {
                            if exclude_globs.iter().any(|g| glob_matches(&m.file_path, g)) { continue; }
                            violations.push(Violation {
                                kind: "structural_must_not_contain_found".to_string(),
                                pattern: entry.clone(),
                                file: m.file_path.clone(),
                                line: Some(m.start_line),
                                context: Some(m.context.clone()),
                            });
                        }
                    }
                    Err(e) => {
                        warn!("Structural search failed for pattern '{}': {}", pattern, e);
                        violations.push(Violation {
                            kind: "structural_check_error".to_string(),
                            pattern: entry.clone(),
                            file: "N/A".to_string(),
                            line: None,
                            context: Some(format!("Error: {}", e)),
                        });
                    }
                }
            }
            None => {
                violations.push(Violation {
                    kind: "invalid_structural_pattern".to_string(),
                    pattern: entry.clone(),
                    file: "N/A".to_string(),
                    line: None,
                    context: Some(
                        "Expected format 'language:pattern' (e.g. 'rust:$X.unwrap()')".to_string(),
                    ),
                });
            }
        }
    }

    // --- Test command check ---
    // Run the user-defined test command (e.g. "cargo test", "pytest").
    // Counts as one additional check in the score denominator.
    let has_test_cmd = case.checks.test_command.is_some();
    if let Some(cmd) = &case.checks.test_command {
        match run_test_command(cmd, project_root).await {
            Ok(None) => {
                // Command exited 0 — passes, no violation
                debug!("test_command '{}' passed", cmd);
            }
            Ok(Some(output)) => {
                violations.push(Violation {
                    kind: "test_command_failed".to_string(),
                    pattern: cmd.clone(),
                    file: "N/A".to_string(),
                    line: None,
                    context: Some(output),
                });
            }
            Err(e) => {
                violations.push(Violation {
                    kind: "test_command_error".to_string(),
                    pattern: cmd.clone(),
                    file: "N/A".to_string(),
                    line: None,
                    context: Some(format!("Failed to run command: {}", e)),
                });
            }
        }
    }

    // --- Scoring ---

    let total_checks = total_literal + total_structural + if has_test_cmd { 1 } else { 0 };
    let score = if total_checks > 0 {
        (1.0 - (violations.len() as f64 / total_checks as f64)).max(0.0)
    } else {
        1.0
    };

    let passed = violations.is_empty();

    debug!(
        "Case '{}': score={:.2}, violations={} (literal={}, structural={}, test_cmd={}), passed={}",
        case.name,
        score,
        violations.len(),
        total_literal,
        total_structural,
        if has_test_cmd { 1 } else { 0 },
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

/// Count unique files referenced in case violations.
fn count_files_in_case(result: &CaseResult) -> usize {
    let mut files: Vec<&str> = result
        .violations
        .iter()
        .map(|v| v.file.as_str())
        .filter(|f| *f != "N/A")
        .collect();
    files.sort();
    files.dedup();
    files.len()
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

/// Run a shell test command and capture its result.
///
/// Returns `Ok(None)` on success (exit code 0), `Ok(Some(output))` on failure,
/// or `Err(_)` if the command could not be launched.
///
/// The command runs under `sh -c` in the project root so the dev can use
/// the same shell aliases they use interactively (e.g. `cargo test`, `pytest -q`).
/// Security note: `test_command` is read from the project's own golden TOML,
/// so it is developer-controlled and therefore trusted.
async fn run_test_command(cmd: &str, project_root: &Path) -> Result<Option<String>> {
    let output = tokio::process::Command::new("sh")
        .arg("-c")
        .arg(cmd)
        .current_dir(project_root)
        .output()
        .await?;

    if output.status.success() {
        return Ok(None);
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let summary = format!(
        "exit code: {}\nstdout (first 500 chars): {}\nstderr (first 500 chars): {}",
        output.status.code().unwrap_or(-1),
        stdout.chars().take(500).collect::<String>(),
        stderr.chars().take(500).collect::<String>(),
    );
    Ok(Some(summary))
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

/// Get the git commit from the last eval run for a golden set.
async fn get_last_eval_commit(db: &Database, golden_set: &str) -> Option<String> {
    let history = crate::eval::history::get_history(db, golden_set, 1)
        .await
        .ok()?;
    history.first().and_then(|entry| entry.git_commit.clone())
}

/// Get list of files changed since a given git commit.
async fn get_changed_files(base_commit: &str) -> Result<Vec<String>> {
    // Basic validation to prevent flag injection in the commit hash
    if base_commit.starts_with('-') || base_commit.contains(' ') {
        return Err(HiefError::SecurityViolation(format!(
            "Invalid base commit for diff: {}",
            base_commit
        )));
    }

    let output = tokio::process::Command::new("git")
        .args(["diff", "--name-only", base_commit, "HEAD", "--"])
        .output()
        .await?;

    if !output.status.success() {
        return Err(HiefError::Other(format!(
            "git diff failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )));
    }

    let files: Vec<String> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|l| !l.is_empty())
        .map(|l| l.to_string())
        .collect();

    debug!(
        "Diff-based eval: {} files changed since {}",
        files.len(),
        base_commit
    );
    Ok(files)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;

    #[test]
    fn test_parse_structural_entry() {
        assert_eq!(
            parse_structural_entry("rust:$X.unwrap()"),
            Some(("rust", "$X.unwrap()"))
        );
        assert_eq!(
            parse_structural_entry("python:import $MOD"),
            Some(("python", "import $MOD"))
        );
        assert_eq!(parse_structural_entry("nocolon"), None);
        assert_eq!(parse_structural_entry(":nopattern"), None);
        assert_eq!(parse_structural_entry("nolang:"), None);
    }

    #[test]
    fn test_glob_matches() {
        assert!(glob_matches("src/main.rs", "*.rs"));
        assert!(glob_matches("src/eval/scorer.rs", "src/**/*.rs"));
        assert!(!glob_matches("tests/test.py", "src/**/*.rs"));
        assert!(glob_matches("README.md", "*.md"));
        assert!(glob_matches("src/main.rs", "src/main.rs"));
    }

    #[tokio::test]
    async fn test_search_literal_punctuation() {
        let db = Database::open_memory().await.unwrap();
        db.conn()
            .execute(
                "INSERT INTO chunks (file_path, language, content, start_line, end_line, content_hash)
                 VALUES ('foo.rs', 'rust', 'let x = v.unwrap();', 0, 0, 'h');",
                (),
            )
            .await
            .unwrap();

        let results = search_literal(&db, ".unwrap()", &[], None, 10)
            .await
            .unwrap();
        assert!(
            !results.is_empty(),
            "literal search should find dot pattern"
        );
    }

    #[tokio::test]
    async fn test_evaluate_case_handles_dot_pattern() {
        let db = Database::open_memory().await.unwrap();
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
                file_patterns: Vec::new(),
                exclude_file_patterns: Vec::new(),
                structural_must_contain: Vec::new(),
                structural_must_not_contain: Vec::new(),
                diff_only: false,
                test_command: None,
            },
        };

        let result = evaluate_case(&db, &case, Path::new("."), None)
            .await
            .unwrap();
        assert!(!result.violations.is_empty());
    }

    #[tokio::test]
    async fn test_evaluate_case_structural_fields_default() {
        let db = Database::open_memory().await.unwrap();
        db.conn()
            .execute(
                "INSERT INTO chunks (file_path, language, content, start_line, end_line, content_hash)
                 VALUES ('foo.rs', 'rust', 'fn main() {}', 0, 0, 'h2');",
                (),
            )
            .await
            .unwrap();

        // No structural checks → no structural violations
        let case = EvalCase {
            id: "t2".to_string(),
            name: "no structural checks".to_string(),
            priority: "medium".to_string(),
            intent: None,
            checks: crate::eval::golden::EvalChecks {
                must_contain: vec!["fn main".to_string()],
                must_not_contain: Vec::new(),
                file_patterns: Vec::new(),
                exclude_file_patterns: Vec::new(),
                structural_must_contain: Vec::new(),
                structural_must_not_contain: Vec::new(),
                diff_only: false,
                test_command: None,
            },
        };

        let result = evaluate_case(&db, &case, Path::new("."), None)
            .await
            .unwrap();
        assert!(result.passed);
        assert_eq!(result.score, 1.0);
    }
}
