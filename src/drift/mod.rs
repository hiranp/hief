//! Documentation drift detection — validates .hief scaffold against the real codebase.
//!
//! Runs zero-token checkers and returns a 0-100 drift score.
//! Score deductions: error (-10), warning (-3), info (-1), floored at 0.
//!
//! # Checkers
//! 1. convention_paths
//! 2. convention_patterns
//! 3. context_staleness (time or commit-age mode)
//! 4. skills_referenced
//! 5. context_completeness
//! 6. patterns_index_sync
//! 7. npm_docs_drift
//! 8. dependency_version_conflicts

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;

use schemars::JsonSchema;
use serde::Serialize;
use tracing::debug;

use crate::config::Config;
use crate::errors::Result;

#[derive(Debug, Clone, Serialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DriftSeverity {
    Error,
    Warning,
    Info,
}

impl DriftSeverity {
    fn deduction(&self) -> i32 {
        match self {
            DriftSeverity::Error => 10,
            DriftSeverity::Warning => 3,
            DriftSeverity::Info => 1,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            DriftSeverity::Error => "error",
            DriftSeverity::Warning => "warning",
            DriftSeverity::Info => "info",
        }
    }
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct DriftIssue {
    pub checker: String,
    pub file: String,
    pub message: String,
    pub severity: DriftSeverity,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct DriftReport {
    pub score: u8,
    pub issues: Vec<DriftIssue>,
    pub checks_run: Vec<String>,
    pub error_count: usize,
    pub warning_count: usize,
    pub info_count: usize,
}

impl DriftReport {
    fn build(issues: Vec<DriftIssue>, checks_run: Vec<String>) -> Self {
        let deduction: i32 = issues.iter().map(|i| i.severity.deduction()).sum();
        let score = (100i32 - deduction).max(0) as u8;
        let error_count = issues
            .iter()
            .filter(|i| i.severity == DriftSeverity::Error)
            .count();
        let warning_count = issues
            .iter()
            .filter(|i| i.severity == DriftSeverity::Warning)
            .count();
        let info_count = issues
            .iter()
            .filter(|i| i.severity == DriftSeverity::Info)
            .count();
        Self {
            score,
            issues,
            checks_run,
            error_count,
            warning_count,
            info_count,
        }
    }
}

pub fn run(project_root: &Path, config: &Config) -> Result<DriftReport> {
    let mut issues = Vec::new();
    let mut checks_run = Vec::new();

    let checkers: &[(&str, fn(&Path, &Config, &mut Vec<DriftIssue>))] = &[
        ("convention_paths", check_convention_paths),
        ("convention_patterns", check_convention_patterns),
        ("context_staleness", check_context_staleness),
        ("skills_referenced", check_skills_referenced),
        ("context_completeness", check_context_completeness),
        ("patterns_index_sync", check_patterns_index_sync),
        ("npm_docs_drift", check_npm_docs_drift),
        (
            "dependency_version_conflicts",
            check_dependency_version_conflicts,
        ),
    ];

    for (name, checker_fn) in checkers {
        checks_run.push((*name).to_string());
        checker_fn(project_root, config, &mut issues);
        debug!("checker '{}' finished (issues={})", name, issues.len());
    }

    Ok(DriftReport::build(issues, checks_run))
}

fn push(issues: &mut Vec<DriftIssue>, checker: &str, file: &str, msg: &str, sev: DriftSeverity) {
    issues.push(DriftIssue {
        checker: checker.to_string(),
        file: file.to_string(),
        message: msg.to_string(),
        severity: sev,
    });
}

fn check_convention_paths(project_root: &Path, _config: &Config, issues: &mut Vec<DriftIssue>) {
    let conv_path = project_root.join(".hief/conventions.toml");
    if !conv_path.exists() {
        push(
            issues,
            "convention_paths",
            ".hief/conventions.toml",
            "conventions.toml not found - run `hief init`",
            DriftSeverity::Warning,
        );
        return;
    }

    let content = match std::fs::read_to_string(&conv_path) {
        Ok(c) => c,
        Err(e) => {
            push(
                issues,
                "convention_paths",
                ".hief/conventions.toml",
                &format!("Cannot read conventions.toml: {}", e),
                DriftSeverity::Error,
            );
            return;
        }
    };

    for (lineno, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        if !trimmed.starts_with("scope") {
            continue;
        }

        let value = trimmed
            .split('=')
            .nth(1)
            .map(|v| v.trim().trim_matches('"').trim_matches('\'').to_string());

        if let Some(glob_pattern) = value {
            if glob_pattern.is_empty() {
                continue;
            }
            let full_glob = project_root.join(&glob_pattern);
            let matched = glob::glob(&full_glob.to_string_lossy())
                .ok()
                .map(|mut it| it.next().is_some())
                .unwrap_or(false);

            if !matched {
                push(
                    issues,
                    "convention_paths",
                    &format!(".hief/conventions.toml:{}", lineno + 1),
                    &format!("scope glob '{}' matches no files", glob_pattern),
                    DriftSeverity::Warning,
                );
            }
        }
    }
}

const KNOWN_LANGUAGES: &[&str] = &["rust", "python", "typescript", "javascript"];

fn check_convention_patterns(project_root: &Path, _config: &Config, issues: &mut Vec<DriftIssue>) {
    let conv_path = project_root.join(".hief/conventions.toml");
    if !conv_path.exists() {
        return;
    }

    let content = match std::fs::read_to_string(&conv_path) {
        Ok(c) => c,
        Err(_) => return,
    };

    let mut current_language: Option<String> = None;

    for (lineno, line) in content.lines().enumerate() {
        let trimmed = line.trim();

        if trimmed.starts_with("language") {
            if let Some(val) = trimmed.split('=').nth(1) {
                current_language =
                    Some(val.trim().trim_matches('"').trim_matches('\'').to_string());
            }
        }

        if !trimmed.starts_with("check_pattern") {
            continue;
        }

        let pattern = trimmed
            .split('=')
            .nth(1)
            .map(|v| v.trim().trim_matches('"').trim_matches('\'').to_string());

        if let Some(pat) = pattern {
            if pat.is_empty() {
                push(
                    issues,
                    "convention_patterns",
                    &format!(".hief/conventions.toml:{}", lineno + 1),
                    "check_pattern is empty",
                    DriftSeverity::Warning,
                );
                continue;
            }

            if let Some(lang) = &current_language
                && !KNOWN_LANGUAGES.contains(&lang.as_str())
            {
                push(
                    issues,
                    "convention_patterns",
                    &format!(".hief/conventions.toml:{}", lineno + 1),
                    &format!(
                        "unsupported language '{}' in check_pattern; valid: {}",
                        lang,
                        KNOWN_LANGUAGES.join(", ")
                    ),
                    DriftSeverity::Warning,
                );
            }
        }
    }
}

fn check_context_staleness(project_root: &Path, config: &Config, issues: &mut Vec<DriftIssue>) {
    let context_dir = project_root.join(".hief/context");
    if !context_dir.exists() {
        return;
    }

    let entries = match std::fs::read_dir(&context_dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    let mode = config.drift.freshness_mode.to_lowercase();
    let threshold_days = config.drift.staleness_days as i64;
    let threshold_commits = config.drift.staleness_commits as i64;

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }

        let rel_path = path
            .strip_prefix(project_root)
            .unwrap_or(&path)
            .to_string_lossy()
            .to_string();

        if mode == "commit" {
            if let Some(commits_since) = git_commits_since_touch(project_root, &rel_path)
                && commits_since > threshold_commits
            {
                push(
                    issues,
                    "context_staleness",
                    &rel_path,
                    &format!(
                        "Not updated in {} commits (threshold: {})",
                        commits_since, threshold_commits
                    ),
                    DriftSeverity::Warning,
                );
            }
            continue;
        }

        let mtime = git_last_commit_time(project_root, &rel_path).or_else(|| fs_mtime(&path));
        if let Some(ts) = mtime {
            let now = now_ts();
            let age_days = (now - ts) / 86_400;
            if age_days > threshold_days {
                push(
                    issues,
                    "context_staleness",
                    &rel_path,
                    &format!(
                        "Not updated in {} days (threshold: {})",
                        age_days, threshold_days
                    ),
                    DriftSeverity::Warning,
                );
            }
        }
    }
}

fn git_last_commit_time(project_root: &Path, file_path: &str) -> Option<i64> {
    let output = Command::new("git")
        .args(["log", "--format=%ct", "-1", "--", file_path])
        .current_dir(project_root)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout.trim().parse::<i64>().ok()
}

fn git_commits_since_touch(project_root: &Path, file_path: &str) -> Option<i64> {
    let last_commit_output = Command::new("git")
        .args(["rev-list", "-n", "1", "HEAD", "--", file_path])
        .current_dir(project_root)
        .output()
        .ok()?;
    if !last_commit_output.status.success() {
        return None;
    }
    let last_commit = String::from_utf8_lossy(&last_commit_output.stdout)
        .trim()
        .to_string();
    if last_commit.is_empty() {
        return None;
    }

    let range = format!("{}..HEAD", last_commit);
    let count_output = Command::new("git")
        .args(["rev-list", "--count", &range])
        .current_dir(project_root)
        .output()
        .ok()?;
    if !count_output.status.success() {
        return None;
    }
    String::from_utf8_lossy(&count_output.stdout)
        .trim()
        .parse::<i64>()
        .ok()
}

fn fs_mtime(path: &Path) -> Option<i64> {
    path.metadata()
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
}

fn now_ts() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn check_skills_referenced(project_root: &Path, config: &Config, issues: &mut Vec<DriftIssue>) {
    let conv_path = project_root.join(".hief/conventions.toml");
    if !conv_path.exists() {
        return;
    }

    let content = match std::fs::read_to_string(&conv_path) {
        Ok(c) => c,
        Err(_) => return,
    };

    let skills_dir = project_root.join(&config.skills.skills_path);

    for (lineno, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        if !trimmed.starts_with("skill") {
            continue;
        }

        let value = trimmed
            .split('=')
            .nth(1)
            .map(|v| v.trim().trim_matches('"').trim_matches('\'').to_string());

        if let Some(skill_name) = value {
            if skill_name.is_empty() {
                continue;
            }
            let exists = ["md", "yaml", "yml", "txt"]
                .iter()
                .any(|ext| skills_dir.join(format!("{}.{}", skill_name, ext)).exists());

            if !exists {
                push(
                    issues,
                    "skills_referenced",
                    &format!(".hief/conventions.toml:{}", lineno + 1),
                    &format!("Referenced skill '{}' not found", skill_name),
                    DriftSeverity::Warning,
                );
            }
        }
    }
}

const REQUIRED_CONTEXT: &[(&str, &str)] = &[
    ("architecture.md", "how components connect"),
    ("conventions.md", "coding patterns"),
    ("setup.md", "how to run and develop"),
];
const RECOMMENDED_CONTEXT: &[(&str, &str)] = &[
    ("stack.md", "technology choices"),
    ("decisions.md", "decision log"),
];

fn check_context_completeness(project_root: &Path, _config: &Config, issues: &mut Vec<DriftIssue>) {
    let context_dir = project_root.join(".hief/context");

    if !context_dir.exists() {
        push(
            issues,
            "context_completeness",
            ".hief/context/",
            "Context directory missing",
            DriftSeverity::Warning,
        );
        return;
    }

    for (filename, purpose) in REQUIRED_CONTEXT {
        if !context_dir.join(filename).exists() {
            push(
                issues,
                "context_completeness",
                &format!(".hief/context/{}", filename),
                &format!("Required context file missing ({})", purpose),
                DriftSeverity::Error,
            );
        }
    }

    for (filename, purpose) in RECOMMENDED_CONTEXT {
        if !context_dir.join(filename).exists() {
            push(
                issues,
                "context_completeness",
                &format!(".hief/context/{}", filename),
                &format!("Recommended context file missing ({})", purpose),
                DriftSeverity::Info,
            );
        }
    }
}

fn check_patterns_index_sync(project_root: &Path, _config: &Config, issues: &mut Vec<DriftIssue>) {
    let patterns_dir = project_root.join(".hief/patterns");
    if !patterns_dir.exists() {
        return;
    }

    let actual_files: Vec<String> = std::fs::read_dir(&patterns_dir)
        .ok()
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            if name.ends_with(".md") && name != "INDEX.md" {
                Some(name)
            } else {
                None
            }
        })
        .collect();

    let index_path = patterns_dir.join("INDEX.md");
    if !index_path.exists() {
        if !actual_files.is_empty() {
            push(
                issues,
                "patterns_index_sync",
                ".hief/patterns/INDEX.md",
                "INDEX.md missing; run `hief patterns sync`",
                DriftSeverity::Warning,
            );
        }
        return;
    }

    let index_content = match std::fs::read_to_string(&index_path) {
        Ok(c) => c,
        Err(_) => return,
    };

    let indexed: Vec<String> = index_content
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if let Some(start) = line.find('(')
                && let Some(end) = line.rfind(')')
                && start < end
            {
                let link = &line[start + 1..end];
                if link.ends_with(".md") && link != "INDEX.md" && !link.contains('/') {
                    return Some(link.to_string());
                }
            }
            None
        })
        .collect();

    for actual in &actual_files {
        if !indexed.contains(actual) {
            push(
                issues,
                "patterns_index_sync",
                &format!(".hief/patterns/{}", actual),
                &format!("Pattern '{}' not listed in INDEX.md", actual),
                DriftSeverity::Warning,
            );
        }
    }

    for indexed_file in &indexed {
        if !patterns_dir.join(indexed_file).exists() {
            push(
                issues,
                "patterns_index_sync",
                ".hief/patterns/INDEX.md",
                &format!("INDEX.md references missing file '{}'", indexed_file),
                DriftSeverity::Error,
            );
        }
    }
}

fn check_npm_docs_drift(project_root: &Path, _config: &Config, issues: &mut Vec<DriftIssue>) {
    let package_json = project_root.join("package.json");
    if !package_json.exists() {
        return;
    }

    let package_content = match std::fs::read_to_string(&package_json) {
        Ok(c) => c,
        Err(_) => return,
    };

    let package_val: serde_json::Value = match serde_json::from_str(&package_content) {
        Ok(v) => v,
        Err(_) => return,
    };

    let scripts_obj = package_val
        .get("scripts")
        .and_then(|v| v.as_object())
        .cloned()
        .unwrap_or_default();

    if scripts_obj.is_empty() {
        return;
    }

    let mut docs_to_scan = vec![
        project_root.join("README.md"),
        project_root.join("AGENTS.md"),
        project_root.join("CLAUDE.md"),
    ];
    collect_markdown_files(&project_root.join("docs"), &mut docs_to_scan);
    collect_markdown_files(&project_root.join(".hief/context"), &mut docs_to_scan);

    for md in docs_to_scan {
        if !md.exists() {
            continue;
        }
        let content = match std::fs::read_to_string(&md) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let refs = extract_script_refs(&content);
        for script in refs {
            if !scripts_obj.contains_key(&script) {
                let rel = md
                    .strip_prefix(project_root)
                    .unwrap_or(&md)
                    .to_string_lossy()
                    .to_string();
                push(
                    issues,
                    "npm_docs_drift",
                    &rel,
                    &format!(
                        "References npm script '{}' not found in package.json",
                        script
                    ),
                    DriftSeverity::Warning,
                );
            }
        }
    }
}

fn check_dependency_version_conflicts(
    project_root: &Path,
    _config: &Config,
    issues: &mut Vec<DriftIssue>,
) {
    let mut packages = Vec::new();
    collect_package_json_files(project_root, &mut packages);
    if packages.len() <= 1 {
        return;
    }

    let mut versions_by_dep: BTreeMap<String, BTreeMap<String, Vec<String>>> = BTreeMap::new();

    for pkg in packages {
        let rel = pkg
            .strip_prefix(project_root)
            .unwrap_or(&pkg)
            .to_string_lossy()
            .to_string();

        let content = match std::fs::read_to_string(&pkg) {
            Ok(c) => c,
            Err(_) => continue,
        };
        let value: serde_json::Value = match serde_json::from_str(&content) {
            Ok(v) => v,
            Err(_) => continue,
        };

        for field in [
            "dependencies",
            "devDependencies",
            "peerDependencies",
            "optionalDependencies",
        ] {
            if let Some(obj) = value.get(field).and_then(|v| v.as_object()) {
                for (dep, ver) in obj {
                    if let Some(ver_str) = ver.as_str() {
                        versions_by_dep
                            .entry(dep.clone())
                            .or_default()
                            .entry(ver_str.to_string())
                            .or_default()
                            .push(format!("{} ({})", rel, field));
                    }
                }
            }
        }
    }

    for (dep, by_version) in versions_by_dep {
        if by_version.len() <= 1 {
            continue;
        }

        let versions: Vec<String> = by_version.keys().cloned().collect();
        let mut locations = Vec::new();
        for (ver, locs) in by_version {
            locations.push(format!("{} -> {}", ver, locs.join(", ")));
        }

        push(
            issues,
            "dependency_version_conflicts",
            "package.json",
            &format!(
                "Dependency '{}' has conflicting versions [{}]. {}",
                dep,
                versions.join(", "),
                locations.join("; ")
            ),
            DriftSeverity::Warning,
        );
    }
}

fn collect_markdown_files(dir: &Path, out: &mut Vec<PathBuf>) {
    if !dir.exists() {
        return;
    }
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                collect_markdown_files(&path, out);
            } else if path.extension().and_then(|e| e.to_str()) == Some("md") {
                out.push(path);
            }
        }
    }
}

fn collect_package_json_files(dir: &Path, out: &mut Vec<PathBuf>) {
    if !dir.exists() {
        return;
    }
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let name = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or_default();
                if [".git", "target", "node_modules"].contains(&name) {
                    continue;
                }
                collect_package_json_files(&path, out);
            } else if path.file_name().and_then(|f| f.to_str()) == Some("package.json") {
                out.push(path);
            }
        }
    }
}

fn extract_script_refs(content: &str) -> BTreeSet<String> {
    let mut refs = BTreeSet::new();
    let tokens: Vec<&str> = content.split_whitespace().collect();

    for i in 0..tokens.len() {
        if tokens[i] == "npm" && i + 2 < tokens.len() && tokens[i + 1] == "run" {
            if let Some(script) = normalize_script_token(tokens[i + 2]) {
                refs.insert(script);
            }
        }
        if tokens[i] == "pnpm" && i + 1 < tokens.len() {
            let maybe_run = tokens[i + 1];
            if maybe_run == "run" {
                if i + 2 < tokens.len()
                    && let Some(script) = normalize_script_token(tokens[i + 2])
                {
                    refs.insert(script);
                }
            } else if let Some(script) = normalize_script_token(maybe_run) {
                refs.insert(script);
            }
        }
        if tokens[i] == "yarn"
            && i + 1 < tokens.len()
            && let Some(script) = normalize_script_token(tokens[i + 1])
        {
            refs.insert(script);
        }
    }

    refs
}

fn normalize_script_token(raw: &str) -> Option<String> {
    let cleaned = raw
        .trim_matches(|c: char| c == '`' || c == '"' || c == '\'' || c == ')' || c == '(')
        .trim_matches(|c: char| c == ',' || c == '.' || c == ';' || c == ':');
    if cleaned.is_empty() {
        return None;
    }
    if cleaned.starts_with('-') {
        return None;
    }
    Some(cleaned.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn run_has_eight_checkers() {
        let root = tempdir().expect("tempdir");
        let cfg = Config::default();
        let report = run(root.path(), &cfg).expect("run");
        assert_eq!(report.checks_run.len(), 8);
    }

    #[test]
    fn severity_counts_match_issues() {
        let root = tempdir().expect("tempdir");
        let cfg = Config::default();
        let report = run(root.path(), &cfg).expect("run");
        let errors = report
            .issues
            .iter()
            .filter(|i| i.severity == DriftSeverity::Error)
            .count();
        assert_eq!(errors, report.error_count);
    }

    #[test]
    fn script_ref_extraction_works() {
        let refs = extract_script_refs("run npm run build and yarn test then pnpm run lint");
        assert!(refs.contains("build"));
        assert!(refs.contains("test"));
        assert!(refs.contains("lint"));
    }

    #[test]
    fn score_floors_at_zero() {
        let issues = (0..20)
            .map(|idx| DriftIssue {
                checker: "x".to_string(),
                file: format!("f{}", idx),
                message: "m".to_string(),
                severity: DriftSeverity::Error,
            })
            .collect::<Vec<_>>();
        let report = DriftReport::build(issues, vec!["x".to_string()]);
        assert_eq!(report.score, 0);
    }
}
