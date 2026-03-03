//! CLI command definitions and dispatch using clap.

pub mod commands;

use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "hief",
    version,
    about = "Hybrid Intent-Evaluation Framework — a sidecar for AI coding agents"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,

    /// Path to config file
    #[arg(short, long, default_value = "hief.toml")]
    pub config: PathBuf,

    /// Verbose output (repeat for more verbosity: -v, -vv, -vvv)
    #[arg(short, long, action = clap::ArgAction::Count)]
    pub verbose: u8,

    /// Output as JSON instead of human-readable text
    #[arg(long)]
    pub json: bool,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Initialize HIEF in the current project
    Init,

    /// Code index operations
    #[command(subcommand)]
    Index(IndexCmd),

    /// Intent graph operations
    #[command(subcommand)]
    Graph(GraphCmd),

    /// Evaluation operations
    #[command(subcommand)]
    Eval(EvalCmd),

    /// Health checks: index staleness, graph integrity, eval drift
    Doctor {
        /// Attempt to auto-fix detected issues
        #[arg(long)]
        fix: bool,
    },

    /// Git hook management
    #[command(subcommand)]
    Hooks(HooksCmd),

    /// Start the MCP server
    Serve(ServeArgs),

    /// Print version information
    Version,
}

#[derive(Subcommand)]
pub enum IndexCmd {
    /// Build or update the code index (incremental)
    Build,
    /// Search indexed code chunks
    Search {
        /// Search query (supports FTS5 syntax: prefix*, "phrases", AND/OR)
        query: String,
        /// Maximum number of results
        #[arg(short = 'k', long, default_value = "10")]
        top_k: usize,
        /// Filter by programming language
        #[arg(short, long)]
        language: Option<String>,
        /// Filter by symbol kind (function, struct, class, etc.)
        #[arg(long)]
        kind: Option<String>,
    },
    /// Show index statistics
    Status,
}

#[derive(Subcommand)]
pub enum GraphCmd {
    /// Create a new intent
    Create {
        /// Kind: feature, bug, refactor, spike, test, chore
        #[arg(short, long)]
        kind: String,
        /// Short title for the intent
        #[arg(short, long)]
        title: String,
        /// Detailed description
        #[arg(short, long)]
        description: Option<String>,
        /// Priority: critical, high, medium, low
        #[arg(short, long, default_value = "medium")]
        priority: String,
        /// Depends on these intent IDs (comma-separated)
        #[arg(long)]
        depends_on: Option<String>,
    },
    /// List intents
    List {
        /// Filter by status
        #[arg(short, long)]
        status: Option<String>,
        /// Filter by kind
        #[arg(short, long)]
        kind: Option<String>,
    },
    /// Show intent details with dependencies
    Show {
        /// Intent ID or prefix (e.g., hief-a1b2 or a1b2)
        id: String,
    },
    /// Update an intent
    Update {
        /// Intent ID or prefix (e.g., hief-a1b2 or a1b2)
        id: String,
        /// New status
        #[arg(short, long)]
        status: Option<String>,
        /// Assign to agent or human
        #[arg(short, long)]
        assign: Option<String>,
    },
    /// Show intents ready for work (all dependencies satisfied)
    Ready,
    /// Validate graph integrity (cycles, orphans, blocked nodes)
    Validate,
}

#[derive(Subcommand)]
pub enum EvalCmd {
    /// Run evaluation against golden sets
    Run {
        /// Specific golden set name (or all if omitted)
        #[arg(long)]
        golden: Option<String>,
        /// CI mode: enforce thresholds, exit code 1 on failure
        #[arg(long)]
        ci: bool,
    },
    /// Show score history and trends
    Report {
        /// Golden set name
        golden: String,
        /// Number of recent entries to show
        #[arg(short, long, default_value = "10")]
        limit: usize,
    },
    /// List available golden sets
    #[command(subcommand)]
    Golden(GoldenCmd),
}

#[derive(Subcommand)]
pub enum GoldenCmd {
    /// List all golden sets
    List,
}

#[derive(Subcommand)]
pub enum HooksCmd {
    /// Install git hooks for auto-indexing and intent sync
    Install,
    /// Remove HIEF git hooks
    Uninstall,
    /// Show current hook status
    Status,
}

#[derive(clap::Args)]
pub struct ServeArgs {
    /// Transport: stdio or http
    #[arg(long)]
    pub transport: Option<String>,
    /// Port for HTTP transport
    #[arg(long)]
    pub port: Option<u16>,
}
