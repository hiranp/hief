//! Issue claims shared by every worktree of one clone.
//!
//! Soft intent locks live in each worktree's own `.hief/hief.db`, so two
//! worktrees never see each other's. Claims are one JSON file per issue under
//! `$(git rev-parse --git-common-dir)/hief/claims/`, a directory all worktrees
//! of the clone share. No daemon and no `flock`: a claim is created by writing
//! a temp file and hard-linking it into place, which fails if the claim exists.
//!
//! Intent locks stay separate. They guard files for one intent inside the
//! database; claims say who works on an issue, across worktrees.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::errors::{HiefError, Result};

/// Label added to a GitHub issue while it is claimed.
pub const IN_PROGRESS_LABEL: &str = "status:in-progress";

/// Who holds a claim.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct Holder {
    pub agent: String,
    pub worktree: String,
    pub branch: String,
}

/// A lease on one issue.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Claim {
    pub issue: String,
    pub holder: Holder,
    /// Unix seconds.
    pub claimed_at: i64,
    /// Unix seconds. After this the lease can be taken over.
    pub expires_at: i64,
}

impl Claim {
    pub fn is_expired(&self, now: i64) -> bool {
        self.expires_at <= now
    }
}

/// Current unix time in seconds.
pub fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Parse a lease such as `90s`, `30m`, `4h` or `2d` into seconds.
pub fn parse_lease(text: &str) -> Result<i64> {
    let text = text.trim();
    let bad = || HiefError::Config(format!("invalid lease '{text}' (use e.g. 30m, 4h, 2d)"));
    let split = text.len().saturating_sub(1);
    if !text.is_char_boundary(split) {
        return Err(bad());
    }
    let (digits, unit) = text.split_at(split);
    let n: i64 = digits.parse().map_err(|_| bad())?;
    let secs = match unit {
        "s" => n,
        "m" => n * 60,
        "h" => n * 3600,
        "d" => n * 86400,
        _ => return Err(bad()),
    };
    if secs <= 0 {
        return Err(bad());
    }
    Ok(secs)
}

fn git(project_root: &Path, args: &[&str]) -> Option<String> {
    let out = Command::new("git")
        .args(args)
        .current_dir(project_root)
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Identify the caller: `agent` (or `$HIEF_AGENT`, or `$USER`), this
/// worktree's top-level path, and the checked-out branch.
pub fn current_holder(project_root: &Path, agent: Option<&str>) -> Holder {
    let agent = agent
        .map(str::to_string)
        .or_else(|| std::env::var("HIEF_AGENT").ok())
        .or_else(|| std::env::var("USER").ok())
        .filter(|a| !a.trim().is_empty())
        .unwrap_or_else(|| "unknown".to_string());
    let worktree = git(project_root, &["rev-parse", "--show-toplevel"])
        .unwrap_or_else(|| project_root.to_string_lossy().into_owned());
    let branch = git(project_root, &["rev-parse", "--abbrev-ref", "HEAD"])
        .unwrap_or_else(|| "unknown".to_string());
    Holder {
        agent,
        worktree,
        branch,
    }
}

/// The claims directory of one clone.
pub struct Claims {
    dir: PathBuf,
}

impl Claims {
    /// Locate the shared claims directory for the clone containing `project_root`.
    pub fn open(project_root: &Path) -> Result<Self> {
        let common = git(
            project_root,
            &["rev-parse", "--path-format=absolute", "--git-common-dir"],
        )
        .ok_or_else(|| HiefError::Config("claims need a git repository".to_string()))?;
        Ok(Self::at(PathBuf::from(common).join("hief").join("claims")))
    }

    fn at(dir: PathBuf) -> Self {
        Self { dir }
    }

    fn path_for(&self, issue: &str) -> Result<PathBuf> {
        let ok = !issue.is_empty()
            && issue.len() <= 64
            && issue
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '#'))
            && !issue.starts_with('.');
        if !ok {
            return Err(HiefError::Config(format!(
                "invalid issue id '{issue}' (letters, digits, - _ . # only)"
            )));
        }
        Ok(self.dir.join(format!("{}.json", issue.replace('#', "_"))))
    }

    fn read(path: &Path) -> Option<Claim> {
        serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
    }

    fn write_tmp(&self, claim: &Claim) -> Result<PathBuf> {
        std::fs::create_dir_all(&self.dir)?;
        let tmp = self.dir.join(format!(
            ".tmp-{}-{}",
            std::process::id(),
            claim.issue.replace('#', "_")
        ));
        let body = serde_json::to_string_pretty(claim)
            .map_err(|e| HiefError::Other(format!("cannot serialize claim: {e}")))?;
        std::fs::write(&tmp, body)?;
        Ok(tmp)
    }

    /// Take (or renew) a claim. Fails if another holder has a live lease.
    pub fn claim(&self, issue: &str, holder: &Holder, lease_secs: i64, now: i64) -> Result<Claim> {
        let path = self.path_for(issue)?;
        let claim = Claim {
            issue: issue.to_string(),
            holder: holder.clone(),
            claimed_at: now,
            expires_at: now + lease_secs,
        };
        let tmp = self.write_tmp(&claim)?;
        let result = self.install(&path, &tmp, &claim, now);
        let _ = std::fs::remove_file(&tmp);
        result.map(|()| claim)
    }

    fn install(&self, path: &Path, tmp: &Path, claim: &Claim, now: i64) -> Result<()> {
        match std::fs::hard_link(tmp, path) {
            Ok(()) => return Ok(()),
            Err(e) if e.kind() != std::io::ErrorKind::AlreadyExists => return Err(e.into()),
            Err(_) => {}
        }
        // A claim exists. An unreadable file counts as abandoned.
        if let Some(existing) = Self::read(path)
            && !existing.is_expired(now)
        {
            if existing.holder == claim.holder {
                // Renewal by the holder: replace in one step.
                std::fs::rename(tmp, path)?;
                return Ok(());
            }
            return Err(conflict(&existing));
        }
        // Expired: remove then link again. If two takers race, one link loses.
        let _ = std::fs::remove_file(path);
        match std::fs::hard_link(tmp, path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Self::read(path)
                .map(|c| Err(conflict(&c)))
                .unwrap_or_else(|| {
                    Err(HiefError::Claim(format!(
                        "{} was just claimed",
                        claim.issue
                    )))
                }),
            Err(e) => Err(e.into()),
        }
    }

    /// Release a claim. Only the holder may, unless `force`.
    pub fn release(&self, issue: &str, holder: &Holder, force: bool) -> Result<Claim> {
        let path = self.path_for(issue)?;
        let existing =
            Self::read(&path).ok_or_else(|| HiefError::Claim(format!("{issue} is not claimed")))?;
        if existing.holder != *holder && !force {
            return Err(HiefError::Claim(format!(
                "{issue} is held by {} ({}); use --force to release it anyway",
                existing.holder.agent, existing.holder.worktree
            )));
        }
        std::fs::remove_file(&path)?;
        Ok(existing)
    }

    /// All claims, expired ones included (check [`Claim::is_expired`]).
    pub fn list(&self) -> Result<Vec<Claim>> {
        let mut claims = Vec::new();
        let Ok(entries) = std::fs::read_dir(&self.dir) else {
            return Ok(claims);
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "json")
                && let Some(claim) = Self::read(&path)
            {
                claims.push(claim);
            }
        }
        claims.sort_by(|a, b| a.issue.cmp(&b.issue));
        Ok(claims)
    }
}

/// Human text for a claim's expiry: `in 3h 20m` or `expired`.
pub fn describe_expiry(claim: &Claim, now: i64) -> String {
    let left = claim.expires_at - now;
    if left <= 0 {
        return "expired".to_string();
    }
    match (left / 3600, (left % 3600) / 60) {
        (0, 0) => format!("in {left}s"),
        (0, m) => format!("in {m}m"),
        (h, m) => format!("in {h}h {m}m"),
    }
}

fn conflict(existing: &Claim) -> HiefError {
    HiefError::Claim(format!(
        "{} is already claimed by {} (worktree {}, branch {}); lease expires {}",
        existing.issue,
        existing.holder.agent,
        existing.holder.worktree,
        existing.holder.branch,
        describe_expiry(existing, unix_now())
    ))
}

/// Mirror a claim on GitHub: add or remove the in-progress label and comment.
/// This is the cross-machine signal; the local file stays the source of truth,
/// so a failure here is reported to the caller but never undoes the claim.
pub fn sync_github(project_root: &Path, claim: &Claim, claimed: bool) -> Result<()> {
    let number = claim.issue.trim_start_matches('#');
    if number.is_empty() || !number.chars().all(|c| c.is_ascii_digit()) {
        return Err(HiefError::Claim(format!(
            "--github needs a numeric issue, got '{}'",
            claim.issue
        )));
    }
    let (label_flag, body) = if claimed {
        (
            "--add-label",
            format!(
                "Claimed by `{}` on branch `{}`.",
                claim.holder.agent, claim.holder.branch
            ),
        )
    } else {
        (
            "--remove-label",
            format!("Released by `{}`.", claim.holder.agent),
        )
    };
    let run = |args: &[&str]| -> Result<()> {
        let out = Command::new("gh")
            .args(args)
            .current_dir(project_root)
            .output()
            .map_err(|e| HiefError::Claim(format!("cannot run gh: {e}")))?;
        if out.status.success() {
            Ok(())
        } else {
            Err(HiefError::Claim(format!(
                "gh {} failed: {}",
                args.join(" "),
                String::from_utf8_lossy(&out.stderr).trim()
            )))
        }
    };
    run(&["issue", "edit", number, label_flag, IN_PROGRESS_LABEL])?;
    run(&["issue", "comment", number, "--body", &body])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn holder(agent: &str, worktree: &str) -> Holder {
        Holder {
            agent: agent.into(),
            worktree: worktree.into(),
            branch: "main".into(),
        }
    }

    fn claims() -> (tempfile::TempDir, Claims) {
        let dir = tempfile::tempdir().unwrap();
        let c = Claims::at(dir.path().join("claims"));
        (dir, c)
    }

    #[test]
    fn expiry_text() {
        let claim = |expires_at| Claim {
            issue: "1".into(),
            holder: holder("a", "w"),
            claimed_at: 0,
            expires_at,
        };
        assert_eq!(describe_expiry(&claim(100), 100), "expired");
        assert_eq!(describe_expiry(&claim(130), 100), "in 30s");
        assert_eq!(describe_expiry(&claim(100 + 600), 100), "in 10m");
        assert_eq!(
            describe_expiry(&claim(100 + 3 * 3600 + 1200), 100),
            "in 3h 20m"
        );
    }

    #[test]
    fn lease_parsing() {
        assert_eq!(parse_lease("90s").unwrap(), 90);
        assert_eq!(parse_lease("30m").unwrap(), 1800);
        assert_eq!(parse_lease("4h").unwrap(), 14400);
        assert_eq!(parse_lease("2d").unwrap(), 172800);
        for bad in ["", "h", "4", "0h", "-1h", "4x", "1.5h", "é"] {
            assert!(parse_lease(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn live_claim_blocks_others_but_not_the_holder() {
        let (_d, c) = claims();
        let a = holder("a", "/wt/a");
        let b = holder("b", "/wt/b");
        c.claim("12", &a, 100, 1000).unwrap();
        let err = c.claim("12", &b, 100, 1050).unwrap_err().to_string();
        assert!(err.contains("already claimed by a"), "{err}");
        // The holder renews, extending the lease.
        let renewed = c.claim("12", &a, 100, 1050).unwrap();
        assert_eq!(renewed.expires_at, 1150);
        assert_eq!(c.list().unwrap().len(), 1);
    }

    #[test]
    fn expired_lease_is_taken_over() {
        let (_d, c) = claims();
        c.claim("12", &holder("a", "/wt/a"), 100, 1000).unwrap();
        let taken = c.claim("12", &holder("b", "/wt/b"), 100, 1100).unwrap();
        assert_eq!(taken.holder.agent, "b");
        assert_eq!(c.list().unwrap()[0].holder.agent, "b");
    }

    #[test]
    fn release_needs_the_holder_or_force() {
        let (_d, c) = claims();
        let a = holder("a", "/wt/a");
        let b = holder("b", "/wt/b");
        c.claim("7", &a, 100, 1000).unwrap();
        assert!(c.release("7", &b, false).is_err());
        assert_eq!(c.list().unwrap().len(), 1);
        c.release("7", &b, true).unwrap();
        assert!(c.list().unwrap().is_empty());
        assert!(c.release("7", &a, false).is_err(), "not claimed any more");
    }

    #[test]
    fn same_agent_in_another_worktree_conflicts() {
        let (_d, c) = claims();
        c.claim("5", &holder("a", "/wt/one"), 100, 1000).unwrap();
        assert!(c.claim("5", &holder("a", "/wt/two"), 100, 1001).is_err());
    }

    #[test]
    fn bad_issue_ids_are_rejected() {
        let (_d, c) = claims();
        for bad in ["", "../x", "a/b", ".hidden", "a b"] {
            assert!(c.claim(bad, &holder("a", "w"), 10, 0).is_err(), "{bad}");
        }
    }

    #[test]
    fn corrupt_claim_file_counts_as_abandoned() {
        let (_d, c) = claims();
        std::fs::create_dir_all(&c.dir).unwrap();
        std::fs::write(c.dir.join("3.json"), "garbage").unwrap();
        c.claim("3", &holder("a", "w"), 10, 0).unwrap();
    }

    /// Two real worktrees of one repo share one claims directory.
    #[test]
    fn two_worktrees_of_one_repo_conflict() {
        let dir = tempfile::tempdir().unwrap();
        let main = dir.path().join("main");
        let second = dir.path().join("second");
        std::fs::create_dir_all(&main).unwrap();
        let git = |cwd: &Path, args: &[&str]| {
            let ok = Command::new("git")
                .args(["-c", "user.email=t@t", "-c", "user.name=t"])
                .args(args)
                .current_dir(cwd)
                .status()
                .unwrap()
                .success();
            assert!(ok, "git {args:?}");
        };
        git(&main, &["init", "-q", "-b", "main"]);
        git(&main, &["commit", "-q", "--allow-empty", "-m", "init"]);
        git(
            &main,
            &[
                "worktree",
                "add",
                "-q",
                "-b",
                "feat",
                second.to_str().unwrap(),
            ],
        );

        let c1 = Claims::open(&main).unwrap();
        let c2 = Claims::open(&second).unwrap();
        assert_eq!(c1.dir, c2.dir);

        let h1 = current_holder(&main, Some("agent-1"));
        let h2 = current_holder(&second, Some("agent-2"));
        assert_ne!(h1.worktree, h2.worktree);
        assert_eq!(h2.branch, "feat");

        let now = unix_now();
        c1.claim("42", &h1, 3600, now).unwrap();
        let err = c2.claim("42", &h2, 3600, now).unwrap_err().to_string();
        assert!(err.contains("agent-1"), "{err}");
        assert!(c2.release("42", &h2, false).is_err());
        c2.release("42", &h2, true).unwrap();
        c2.claim("42", &h2, 3600, now).unwrap();
    }
}
