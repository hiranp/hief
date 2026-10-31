//! `hief claim`, `hief release`, `hief claims` — issue claims shared by all worktrees.

use std::path::Path;

use crate::claims::{self, Claim, Claims};
use crate::config::Config;
use crate::errors::Result;

/// Claim `issue` for the current agent and worktree.
pub fn claim_issue(
    project_root: &Path,
    config: &Config,
    issue: &str,
    lease: Option<&str>,
    agent: Option<&str>,
    github: bool,
    json: bool,
) -> Result<()> {
    let lease_secs = claims::parse_lease(lease.unwrap_or(&config.claims.default_lease))?;
    let holder = claims::current_holder(project_root, agent);
    let claim =
        Claims::open(project_root)?.claim(issue, &holder, lease_secs, claims::unix_now())?;
    let warning = github_sync(project_root, config, github, &claim, true);
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&claim).unwrap_or_default()
        );
    } else {
        println!(
            "Claimed {} for {} (lease expires {}, branch {})",
            claim.issue,
            claim.holder.agent,
            claims::describe_expiry(&claim, claims::unix_now()),
            claim.holder.branch
        );
    }
    finish(warning)
}

/// Release `issue`. Only the holder may, unless `force`.
pub fn release_issue(
    project_root: &Path,
    config: &Config,
    issue: &str,
    agent: Option<&str>,
    force: bool,
    github: bool,
    json: bool,
) -> Result<()> {
    let holder = claims::current_holder(project_root, agent);
    let claim = Claims::open(project_root)?.release(issue, &holder, force)?;
    let warning = github_sync(project_root, config, github, &claim, false);
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&claim).unwrap_or_default()
        );
    } else {
        println!("Released {}", claim.issue);
    }
    finish(warning)
}

/// Print every claim with its holder and expiry.
pub fn list_claims(project_root: &Path, json: bool) -> Result<()> {
    let all = Claims::open(project_root)?.list()?;
    if json {
        println!("{}", serde_json::to_string_pretty(&all).unwrap_or_default());
        return Ok(());
    }
    if all.is_empty() {
        println!("No claims.");
        return Ok(());
    }
    let now = claims::unix_now();
    for c in &all {
        println!(
            "{:<10} {:<16} {:<24} {}  [{}]",
            c.issue,
            c.holder.agent,
            c.holder.branch,
            c.holder.worktree,
            claims::describe_expiry(c, now)
        );
    }
    Ok(())
}

fn github_sync(
    project_root: &Path,
    config: &Config,
    flag: bool,
    claim: &Claim,
    claimed: bool,
) -> Option<String> {
    if !(flag || config.claims.github) {
        return None;
    }
    claims::sync_github(project_root, claim, claimed)
        .err()
        .map(|e| e.to_string())
}

/// The local claim already succeeded; a GitHub failure is reported, not fatal.
fn finish(warning: Option<String>) -> Result<()> {
    if let Some(w) = warning {
        eprintln!("warning: local claim updated, but GitHub was not: {w}");
    }
    Ok(())
}
