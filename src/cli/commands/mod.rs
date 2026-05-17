//! CLI command implementations.
//!
//! Each domain area lives in its own submodule.  This top-level module
//! re-exports every public function and type so that call-sites
//! (`main.rs`, tests, etc.) can continue to use `cli::commands::*`
//! without any path changes.

use std::path::Path;

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

/// Runs the Phase 02 installer command and prints a deterministic preview.
pub fn install_platform(
	config: &crate::config::Config,
	project_root: &Path,
	platform: &str,
	dry_run: bool,
	json: bool,
) -> crate::errors::Result<()> {
	let preview = build_install_preview(config, project_root, platform, dry_run)?;

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

	// TODO(PRO-04): implement real registration write path.
	Ok(())
}
