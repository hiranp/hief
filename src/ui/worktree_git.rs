use std::path::{Path, PathBuf};

use serde::Serialize;
use tokio::process::Command;
use tokio::time::{timeout, Duration};

use crate::errors::{HiefError, Result};

const GIT_TIMEOUT: Duration = Duration::from_secs(3);

#[derive(Debug, Clone, Serialize)]
pub struct GitWorktreeRow {
    pub path: String,
    pub head: Option<String>,
    pub branch: Option<String>,
    pub locked: bool,
    pub prunable: bool,
}

pub async fn list_worktrees(project_root: &Path) -> Result<Vec<GitWorktreeRow>> {
    let stdout = run_git(project_root, &["worktree", "list", "--porcelain"]).await?;
    parse_porcelain(&stdout)
}

pub async fn create_worktree(project_root: &Path, path: &Path, branch: &str) -> Result<()> {
    let path_str = path.to_string_lossy().to_string();
    let args = ["worktree", "add", "-b", branch, path_str.as_str()];
    let _ = run_git(project_root, &args).await?;
    Ok(())
}

pub async fn lock_worktree(project_root: &Path, path: &Path, reason: &str) -> Result<()> {
    let path_str = path.to_string_lossy().to_string();
    let args = ["worktree", "lock", "--reason", reason, path_str.as_str()];
    let _ = run_git(project_root, &args).await?;
    Ok(())
}

pub async fn prune_worktrees(project_root: &Path) -> Result<()> {
    let _ = run_git(project_root, &["worktree", "prune"]).await?;
    Ok(())
}

pub async fn remove_worktree(project_root: &Path, path: &Path, force: bool) -> Result<()> {
    if !force && is_dirty(path).await? {
        return Err(HiefError::Other(format!(
            "cannot remove dirty worktree without force: {}",
            path.display()
        )));
    }

    let path_str = path.to_string_lossy().to_string();
    if force {
        let args = ["worktree", "remove", "--force", path_str.as_str()];
        let _ = run_git(project_root, &args).await?;
    } else {
        let args = ["worktree", "remove", path_str.as_str()];
        let _ = run_git(project_root, &args).await?;
    }
    Ok(())
}

pub async fn repair_worktrees(project_root: &Path) -> Result<()> {
    let _ = run_git(project_root, &["worktree", "repair"]).await?;
    Ok(())
}

pub fn parse_porcelain(stdout: &str) -> Result<Vec<GitWorktreeRow>> {
    let mut rows = Vec::new();
    let mut current: Option<GitWorktreeRow> = None;

    for raw_line in stdout.lines() {
        let line = raw_line.trim();
        if line.is_empty() {
            if let Some(entry) = current.take() {
                rows.push(entry);
            }
            continue;
        }

        if let Some(path) = line.strip_prefix("worktree ") {
            if let Some(entry) = current.take() {
                rows.push(entry);
            }
            current = Some(GitWorktreeRow {
                path: path.to_string(),
                head: None,
                branch: None,
                locked: false,
                prunable: false,
            });
            continue;
        }

        let Some(entry) = current.as_mut() else {
            return Err(HiefError::Other("invalid porcelain output: missing worktree header".to_string()));
        };

        if let Some(head) = line.strip_prefix("HEAD ") {
            entry.head = Some(head.to_string());
        } else if let Some(branch) = line.strip_prefix("branch refs/heads/") {
            entry.branch = Some(branch.to_string());
        } else if line.starts_with("locked") {
            entry.locked = true;
        } else if line.starts_with("prunable") {
            entry.prunable = true;
        }
    }

    if let Some(entry) = current.take() {
        rows.push(entry);
    }

    Ok(rows)
}

pub fn join_worktree_path(project_root: &Path, input: &str) -> PathBuf {
    let candidate = PathBuf::from(input);
    if candidate.is_absolute() {
        candidate
    } else {
        project_root.join(candidate)
    }
}

async fn is_dirty(path: &Path) -> Result<bool> {
    let output = timeout(
        GIT_TIMEOUT,
        Command::new("git")
            .arg("-C")
            .arg(path)
            .arg("status")
            .arg("--porcelain")
            .output(),
    )
    .await
    .map_err(|_| HiefError::Other(format!("git status timed out for {}", path.display())))?
    .map_err(|e| HiefError::Other(format!("failed to run git status: {}", e)))?;

    if !output.status.success() {
        return Err(HiefError::Other(format!(
            "git status failed for {}: {}",
            path.display(),
            String::from_utf8_lossy(&output.stderr)
        )));
    }

    Ok(!String::from_utf8_lossy(&output.stdout).trim().is_empty())
}

async fn run_git(project_root: &Path, args: &[&str]) -> Result<String> {
    let output = timeout(
        GIT_TIMEOUT,
        Command::new("git")
            .arg("-C")
            .arg(project_root)
            .args(args)
            .output(),
    )
    .await
    .map_err(|_| HiefError::Other(format!("git command timed out: git {}", args.join(" "))))?
    .map_err(|e| HiefError::Other(format!("failed to run git command: {}", e)))?;

    if !output.status.success() {
        return Err(HiefError::Other(format!(
            "git command failed (git {}): {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        )));
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}
