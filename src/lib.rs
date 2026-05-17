//! HIEF library entrypoint.
//!
//! Exposes the internal modules so integration tests and external tooling can
//! reuse the same routing and indexing logic as the binary.

pub mod cli;
pub mod config;
pub mod context;
pub mod db;
pub mod docs;
pub mod drift;
pub mod errors;
pub mod eval;
pub mod graph;
pub mod index;
pub mod mcp;
pub mod patterns;
pub mod router;
pub mod skills;
pub mod watcher;
