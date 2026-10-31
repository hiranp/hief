//! Drift baseline and diff scope, so `hief check` can gate CI on a repo that
//! already has drift: only issues that are new count.
//!
//! A baseline entry is `(checker, file, fingerprint)`. The file loses any
//! `:line` suffix and the fingerprint is a hash of the message, so moving code
//! around does not make a known issue look new.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Serialize};

use super::{DriftIssue, DriftReport};
use crate::errors::{HiefError, Result};

/// Default baseline location, relative to the project root.
pub const BASELINE_PATH: &str = ".hief/drift-baseline.json";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
struct Entry {
    checker: String,
    file: String,
    fingerprint: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct BaselineFile {
    version: u32,
    issues: Vec<Entry>,
}

/// Path of the baseline file for a project.
pub fn baseline_path(project_root: &Path) -> PathBuf {
    project_root.join(BASELINE_PATH)
}

/// Strip a trailing `:<line>` from an issue's file field.
pub fn file_without_line(file: &str) -> &str {
    match file.rsplit_once(':') {
        Some((path, line)) if !line.is_empty() && line.bytes().all(|b| b.is_ascii_digit()) => path,
        _ => file,
    }
}

/// The `:<line>` suffix of an issue's file field, if it has one.
pub fn line_of(file: &str) -> Option<u32> {
    file.rsplit_once(':')
        .and_then(|(_, line)| line.parse().ok())
}

fn entry_of(issue: &DriftIssue) -> Entry {
    let hash = blake3::hash(issue.message.as_bytes()).to_hex();
    Entry {
        checker: issue.checker.clone(),
        file: file_without_line(&issue.file).to_string(),
        fingerprint: hash.as_str()[..16].to_string(),
    }
}

/// Write the current issues as the new baseline. Fixed issues drop out.
pub fn write(project_root: &Path, report: &DriftReport) -> Result<usize> {
    let mut issues: Vec<Entry> = report.issues.iter().map(entry_of).collect();
    issues.sort();
    issues.dedup();
    let count = issues.len();

    let path = baseline_path(project_root);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let body = serde_json::to_string_pretty(&BaselineFile { version: 1, issues })
        .map_err(|e| HiefError::Other(format!("cannot serialize baseline: {e}")))?;
    std::fs::write(&path, body + "\n")?;
    Ok(count)
}

/// Load the baseline, or `None` when the file does not exist.
fn load(project_root: &Path) -> Result<Option<BTreeSet<Entry>>> {
    let path = baseline_path(project_root);
    if !path.exists() {
        return Ok(None);
    }
    let raw = std::fs::read_to_string(&path)?;
    let parsed: BaselineFile = serde_json::from_str(&raw)
        .map_err(|e| HiefError::Config(format!("invalid baseline {}: {e}", path.display())))?;
    Ok(Some(parsed.issues.into_iter().collect()))
}

/// Drop issues that are in the baseline. Returns the report and how many were
/// suppressed. With no baseline file, the report is returned unchanged.
pub fn apply(project_root: &Path, report: DriftReport) -> Result<(DriftReport, usize)> {
    let Some(known) = load(project_root)? else {
        return Ok((report, 0));
    };
    let before = report.issues.len();
    let issues: Vec<DriftIssue> = report
        .issues
        .into_iter()
        .filter(|issue| !known.contains(&entry_of(issue)))
        .collect();
    let suppressed = before - issues.len();
    Ok((DriftReport::build(issues, report.checks_run), suppressed))
}

/// Keep only issues in files changed since `base` (`git diff --name-only base...HEAD`).
pub fn diff_scope(project_root: &Path, report: DriftReport, base: &str) -> Result<DriftReport> {
    let output = Command::new("git")
        .args(["diff", "--name-only", &format!("{base}...HEAD")])
        .current_dir(project_root)
        .output()
        .map_err(|e| HiefError::Config(format!("cannot run git: {e}")))?;
    if !output.status.success() {
        return Err(HiefError::Config(format!(
            "git diff {base}...HEAD failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    let changed: BTreeSet<String> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::to_string)
        .collect();
    let issues = report
        .issues
        .into_iter()
        .filter(|issue| changed.contains(file_without_line(&issue.file)))
        .collect();
    Ok(DriftReport::build(issues, report.checks_run))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drift::DriftSeverity;

    fn issue(file: &str, message: &str) -> DriftIssue {
        DriftIssue {
            checker: "c".into(),
            file: file.into(),
            message: message.into(),
            severity: DriftSeverity::Error,
        }
    }

    fn report(issues: Vec<DriftIssue>) -> DriftReport {
        DriftReport::build(issues, vec!["c".into()])
    }

    #[test]
    fn line_numbers_are_not_part_of_the_key() {
        assert_eq!(file_without_line("a/b.md:12"), "a/b.md");
        assert_eq!(file_without_line("a/b.md"), "a/b.md");
        assert_eq!(line_of("a/b.md:12"), Some(12));
        assert_eq!(
            entry_of(&issue("x.md:1", "m")),
            entry_of(&issue("x.md:99", "m"))
        );
    }

    #[test]
    fn baseline_hides_known_issues_and_shows_new_ones() {
        let dir = tempfile::tempdir().unwrap();
        let old = report(vec![issue("a.md:3", "old problem")]);
        assert_eq!(write(dir.path(), &old).unwrap(), 1);

        let now = report(vec![
            issue("a.md:40", "old problem"),
            issue("b.md", "new problem"),
        ]);
        let (filtered, suppressed) = apply(dir.path(), now).unwrap();
        assert_eq!(suppressed, 1);
        assert_eq!(filtered.issues.len(), 1);
        assert_eq!(filtered.issues[0].file, "b.md");
        assert_eq!(filtered.error_count, 1);
    }

    #[test]
    fn rewriting_the_baseline_drops_fixed_issues() {
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            &report(vec![issue("a.md", "m1"), issue("b.md", "m2")]),
        )
        .unwrap();
        write(dir.path(), &report(vec![issue("b.md", "m2")])).unwrap();
        let raw = std::fs::read_to_string(baseline_path(dir.path())).unwrap();
        assert!(!raw.contains("a.md"));
    }

    #[test]
    fn no_baseline_file_means_nothing_is_suppressed() {
        let dir = tempfile::tempdir().unwrap();
        let (r, n) = apply(dir.path(), report(vec![issue("a.md", "m")])).unwrap();
        assert_eq!((r.issues.len(), n), (1, 0));
    }

    #[test]
    fn corrupt_baseline_is_a_config_error() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".hief")).unwrap();
        std::fs::write(baseline_path(dir.path()), "not json").unwrap();
        assert!(matches!(
            apply(dir.path(), report(vec![])),
            Err(HiefError::Config(_))
        ));
    }

    #[test]
    fn diff_scope_keeps_only_changed_files() {
        let dir = tempfile::tempdir().unwrap();
        let git = |args: &[&str]| {
            let ok = Command::new("git")
                .args(["-c", "user.email=t@t", "-c", "user.name=t"])
                .args(args)
                .current_dir(dir.path())
                .status()
                .unwrap()
                .success();
            assert!(ok, "git {args:?}");
        };
        git(&["init", "-q", "-b", "main"]);
        std::fs::write(dir.path().join("a.md"), "a").unwrap();
        std::fs::write(dir.path().join("b.md"), "b").unwrap();
        git(&["add", "."]);
        git(&["commit", "-qm", "base"]);
        std::fs::write(dir.path().join("b.md"), "b2").unwrap();
        git(&["commit", "-qam", "change b"]);

        let r = report(vec![issue("a.md:1", "m"), issue("b.md:2", "m")]);
        let scoped = diff_scope(dir.path(), r, "HEAD~1").unwrap();
        assert_eq!(scoped.issues.len(), 1);
        assert_eq!(scoped.issues[0].file, "b.md:2");
        assert!(diff_scope(dir.path(), report(vec![]), "no-such-ref").is_err());
    }
}
