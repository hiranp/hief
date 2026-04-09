//! CLI command implementations.
//!
//! Each domain area lives in its own submodule.  This top-level module
//! re-exports every public function and type so that call-sites
//! (`main.rs`, tests, etc.) can continue to use `cli::commands::*`
//! without any path changes.

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
