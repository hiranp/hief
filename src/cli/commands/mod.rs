//! CLI command implementations.
//!
//! Each domain area lives in its own submodule.  This top-level module
//! re-exports every public function and type so that call-sites
//! (`main.rs`, tests, etc.) can continue to use `cli::commands::*`
//! without any path changes.

use std::path::Path;

use schemars::JsonSchema;

mod check;
mod docs;
mod doctor;
mod eval;
mod graph;
mod hooks;
mod index;
mod init;
pub mod mcp;
mod patterns;
mod skills;
mod sync;
mod upgrade;
mod watch;

use serde::Serialize;

// Re-export everything so existing `cli::commands::*` paths keep working.
pub use check::*;
pub use docs::*;
pub use doctor::*;
pub use eval::*;
pub use graph::*;
pub use hooks::*;
pub use index::*;
pub use init::*;
pub use mcp::{mcp_install, mcp_status, mcp_uninstall};
pub use patterns::*;
pub use skills::*;
pub use sync::*;
pub use upgrade::*;
pub use watch::*;

/// Supported targets for the Phase 02 installer command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum InstallPlatform {
    ClaudeDesktop,
    Cursor,
    Zed,
    Custom,
}

impl InstallPlatform {
    /// Parses a stable platform name used by `hief install --platform`.
    pub fn parse(raw: &str) -> crate::errors::Result<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "claude-desktop" => Ok(Self::ClaudeDesktop),
            "cursor" => Ok(Self::Cursor),
            "zed" => Ok(Self::Zed),
            "custom" => Ok(Self::Custom),
            other => Err(crate::errors::HiefError::ToolParameterConstraint {
                tool: "install".to_string(),
                parameter: "platform".to_string(),
                constraint: "must be one of claude-desktop|cursor|zed|custom".to_string(),
                actual: other.to_string(),
            }),
        }
    }

    /// Returns the stable CLI string for this platform.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ClaudeDesktop => "claude-desktop",
            Self::Cursor => "cursor",
            Self::Zed => "zed",
            Self::Custom => "custom",
        }
    }
}

/// Deterministic preview returned by `hief install` during the dry-run-only phase.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct InstallPreview {
    pub platform: String,
    pub dry_run: bool,
    pub lane: String,
    pub reason: String,
    pub deferred: bool,
    pub config_block: String,
    pub message: String,
}

/// Builds a deterministic install preview without mutating editor configuration.
pub fn build_install_preview(
    config: &crate::config::Config,
    project_root: &Path,
    platform: &str,
    dry_run: bool,
) -> crate::errors::Result<InstallPreview> {
    let platform = InstallPlatform::parse(platform)?;
    let operation = crate::router::OperationRequest::remote("install-platform", 256);
    let lane = crate::router::select_lane(&operation, &config.router);
    let project_display = project_root.display().to_string();
    let config_block = format!(
        "[mcp_servers.hief]\ncommand = \"hief\"\nargs = [\"serve\"]\nworking_directory = \"{}\"\nplatform = \"{}\"",
        project_display,
        platform.as_str()
    );
    let message = if dry_run {
        format!(
            "dry-run preview for {} registration; no files were changed",
            platform.as_str()
        )
    } else {
        "execution accepted but deferred until the follow-on registration plan".to_string()
    };

    Ok(InstallPreview {
        platform: platform.as_str().to_string(),
        dry_run,
        lane: lane.lane.as_str().to_string(),
        reason: lane.reason,
        deferred: !dry_run,
        config_block,
        message,
    })
}

/// Runs the Phase 02 installer command.
///
/// When `dry_run` is true, prints a deterministic preview without writing any files.
/// When `dry_run` is false, delegates to [`super::mcp::mcp_install`] which performs
/// the real JSON config write for the requested platform (PRO-04).
pub fn install_platform(
    config: &crate::config::Config,
    project_root: &Path,
    platform: &str,
    dry_run: bool,
    json: bool,
) -> crate::errors::Result<()> {
    let preview = build_install_preview(config, project_root, platform, dry_run)?;

    if dry_run {
        // Dry-run: print the preview and return without touching any file.
        if json {
            println!(
                "{}",
                serde_json::to_string_pretty(&preview)
                    .map_err(|e| crate::errors::HiefError::Other(e.to_string()))?
            );
        } else {
            println!("Platform: {}", preview.platform);
            println!("Lane: {}", preview.lane);
            println!("Reason: {}", preview.reason);
            println!("Dry run: {}", preview.dry_run);
            println!("Deferred: {}", preview.deferred);
            println!("{}", preview.message);
            println!();
            println!("{}", preview.config_block);
        }
        return Ok(());
    }

    // Real write path (PRO-04): delegate to mcp_install which already has the
    // full per-platform JSON config write logic, idempotency checks, and
    // platform-specific scope rules.
    //
    // `InstallPlatform::Custom` maps to the "all" target so users get
    // coverage across all known clients when they haven't specified a single one.
    let mcp_target = match preview.platform.as_str() {
        "claude-desktop" => "claude-desktop",
        "cursor" => "cursor",
        "zed" => "all", // Zed not yet in McpClient; fall through to all for best-effort
        "custom" => "all",
        other => other,
    };

    mcp::mcp_install(project_root, Some(mcp_target), false, json)
}

/// Human-facing session telemetry summary for CLI.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct SessionCostSummaryView {
    pub session_id: String,
    pub total_calls: i64,
    pub total_latency_ms: i64,
    pub avg_groundedness: Option<f64>,
    pub per_tool: Vec<SessionCostToolView>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct SessionCostToolView {
    pub tool: String,
    pub total_calls: i64,
    pub total_latency_ms: i64,
    pub avg_groundedness: Option<f64>,
}

/// Render a per-session telemetry summary.
pub async fn session_cost(
    db: &crate::db::Database,
    project_root: &std::path::Path,
    session_id: &str,
    json: bool,
) -> crate::errors::Result<()> {
    let worktree_id = crate::scope::derive_worktree_id(project_root);
    let summary = db
        .get_session_cost_summary_scoped(session_id, Some(&worktree_id))
        .await?;
    let view = SessionCostSummaryView {
        session_id: summary.session_id,
        total_calls: summary.total_calls,
        total_latency_ms: summary.total_latency_ms,
        avg_groundedness: summary.avg_groundedness,
        per_tool: summary
            .per_tool
            .into_iter()
            .map(|tool| SessionCostToolView {
                tool: tool.tool,
                total_calls: tool.total_calls,
                total_latency_ms: tool.total_latency_ms,
                avg_groundedness: tool.avg_groundedness,
            })
            .collect(),
    };

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&view)
                .map_err(|e| crate::errors::HiefError::Other(e.to_string()))?
        );
        return Ok(());
    }

    println!("Session: {}", view.session_id);
    println!("Total calls: {}", view.total_calls);
    println!("Total latency (ms): {}", view.total_latency_ms);
    match view.avg_groundedness {
        Some(score) => println!("Average groundedness: {:.3}", score),
        None => println!("Average groundedness: n/a"),
    }

    if view.per_tool.is_empty() {
        println!("Per-tool breakdown: none");
        return Ok(());
    }

    println!("Per-tool breakdown:");
    for row in view.per_tool {
        let groundedness = row
            .avg_groundedness
            .map(|score| format!("{:.3}", score))
            .unwrap_or_else(|| "n/a".to_string());
        println!(
            "- {}: calls={}, latency_ms={}, avg_groundedness={}",
            row.tool, row.total_calls, row.total_latency_ms, groundedness
        );
    }

    Ok(())
}
