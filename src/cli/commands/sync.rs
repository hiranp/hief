//! `hief sync` — targeted drift repair prompt generation + pluggable fixer backend.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::config::Config;
use crate::drift::{DriftIssue, DriftReport};
use crate::errors::{HiefError, Result};

pub async fn run_sync(
    project_root: &Path,
    config: &Config,
    backend: Option<&str>,
    apply: bool,
    json: bool,
) -> Result<i32> {
    let before = crate::drift::run(project_root, config)?;
    let prompt = build_targeted_prompt(project_root, &before);
    let prompt_path = write_prompt(project_root, &prompt)?;

    let selected_backend = backend
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| config.sync.default_backend.clone());

    let mut backend_exit: Option<i32> = None;
    let mut backend_error: Option<String> = None;

    if apply && selected_backend != "none" {
        match run_backend(
            project_root,
            config,
            &selected_backend,
            &prompt,
            &prompt_path,
        ) {
            Ok(code) => backend_exit = Some(code),
            Err(e) => backend_error = Some(e.to_string()),
        }
    }

    let after = crate::drift::run(project_root, config)?;

    if json {
        println!(
            "{}",
            serde_json::json!({
                "prompt_path": prompt_path.display().to_string(),
                "backend": selected_backend,
                "apply": apply,
                "backend_exit": backend_exit,
                "backend_error": backend_error,
                "before": before,
                "after": after,
            })
        );
    } else {
        println!("Targeted sync prompt: {}", prompt_path.display());
        println!(
            "Drift score: {} -> {} (errors: {} -> {})",
            before.score, after.score, before.error_count, after.error_count
        );
        if apply {
            if let Some(err) = backend_error {
                eprintln!("Backend '{}' failed: {}", selected_backend, err);
            } else if let Some(code) = backend_exit {
                println!("Backend '{}' exited with code {}", selected_backend, code);
            }
        } else {
            println!(
                "Run with --apply to execute backend '{}' automatically.",
                selected_backend
            );
        }
    }

    Ok(if after.error_count > 0 { 1 } else { 0 })
}

fn write_prompt(project_root: &Path, prompt: &str) -> Result<PathBuf> {
    let dir = project_root.join(".hief").join("tmp");
    std::fs::create_dir_all(&dir)?;
    let path = dir.join("sync-prompt.md");
    std::fs::write(&path, prompt)?;
    Ok(path)
}

fn run_backend(
    project_root: &Path,
    config: &Config,
    backend: &str,
    prompt: &str,
    prompt_path: &Path,
) -> Result<i32> {
    match backend {
        "claude" => run_cmd(project_root, "claude", &["-p", prompt], prompt_path, prompt),
        "codex" => run_cmd(project_root, "codex", &[prompt], prompt_path, prompt),
        "custom" => {
            let command = config.sync.custom_command.as_deref().ok_or_else(|| {
                HiefError::Other("sync.custom_command is not configured".to_string())
            })?;
            run_shell(project_root, command, prompt_path, prompt)
        }
        "none" => Ok(0),
        other => Err(HiefError::Other(format!(
            "unknown backend '{}' (use none|claude|codex|custom)",
            other
        ))),
    }
}

fn run_cmd(
    project_root: &Path,
    bin: &str,
    args: &[&str],
    prompt_path: &Path,
    prompt: &str,
) -> Result<i32> {
    let status = Command::new(bin)
        .args(args)
        .current_dir(project_root)
        .env("HIEF_SYNC_PROMPT_FILE", prompt_path)
        .env("HIEF_SYNC_PROMPT", prompt)
        .status()
        .map_err(|e| HiefError::Other(format!("failed to run '{}': {}", bin, e)))?;
    Ok(status.code().unwrap_or(1))
}

fn run_shell(project_root: &Path, command: &str, prompt_path: &Path, prompt: &str) -> Result<i32> {
    let status = Command::new("sh")
        .arg("-c")
        .arg(command)
        .current_dir(project_root)
        .env("HIEF_SYNC_PROMPT_FILE", prompt_path)
        .env("HIEF_SYNC_PROMPT", prompt)
        .status()
        .map_err(|e| HiefError::Other(format!("failed to run custom backend: {}", e)))?;
    Ok(status.code().unwrap_or(1))
}

fn build_targeted_prompt(project_root: &Path, report: &DriftReport) -> String {
    let mut by_file: std::collections::BTreeMap<String, Vec<&DriftIssue>> =
        std::collections::BTreeMap::new();
    for issue in &report.issues {
        by_file.entry(issue.file.clone()).or_default().push(issue);
    }

    let mut out = String::new();
    out.push_str("# HIEF Sync Prompt\n\n");
    out.push_str("You are fixing scaffold drift in this repository.\n");
    out.push_str("Only edit files implicated below. Keep changes minimal and deterministic.\n\n");
    out.push_str(&format!("Project root: {}\n", project_root.display()));
    out.push_str(&format!(
        "Current score: {}/100 ({} errors, {} warnings, {} info)\n\n",
        report.score, report.error_count, report.warning_count, report.info_count
    ));

    out.push_str("## Issues by file\n\n");
    if by_file.is_empty() {
        out.push_str("No issues found.\n");
    } else {
        for (file, issues) in &by_file {
            out.push_str(&format!("### {}\n", file));
            for issue in issues {
                out.push_str(&format!(
                    "- [{}] {}: {}\n",
                    issue.severity.label(),
                    issue.checker,
                    issue.message
                ));
            }
            out.push('\n');
        }
    }

    out.push_str("## Constraints\n\n");
    out.push_str("- Do not change unrelated code.\n");
    out.push_str("- Preserve project conventions.\n");
    out.push_str("- Prefer editing scaffold/docs over product code unless issue says otherwise.\n");
    out.push_str("- After edits, run `hief check --json` and ensure score improves.\n");

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_contains_issue_groups() {
        let report = DriftReport {
            score: 70,
            issues: vec![
                DriftIssue {
                    checker: "context_completeness".to_string(),
                    file: ".hief/context/setup.md".to_string(),
                    message: "missing".to_string(),
                    severity: crate::drift::DriftSeverity::Error,
                },
                DriftIssue {
                    checker: "patterns_index_sync".to_string(),
                    file: ".hief/patterns/INDEX.md".to_string(),
                    message: "stale".to_string(),
                    severity: crate::drift::DriftSeverity::Warning,
                },
            ],
            checks_run: vec![],
            error_count: 1,
            warning_count: 1,
            info_count: 0,
        };

        let prompt = build_targeted_prompt(Path::new("/tmp/repo"), &report);
        assert!(prompt.contains(".hief/context/setup.md"));
        assert!(prompt.contains("patterns_index_sync"));
    }
}
