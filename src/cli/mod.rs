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

    /// Documentation drift detection — validate scaffold against codebase (0–100 score)
    Check {
        /// Only print the one-line score summary
        #[arg(long)]
        quiet: bool,
    },

    /// Pattern library management (project-scoped task guides)
    #[command(subcommand)]
    Patterns(PatternsCmd),

    /// Repair scaffold drift using targeted prompts and optional fixer backend
    Sync {
        /// Backend to invoke: none, claude, codex, or custom
        #[arg(long)]
        backend: Option<String>,
        /// Execute the selected backend after generating the prompt
        #[arg(long)]
        apply: bool,
    },

    /// Watch filesystem changes and emit implicit conflict warnings
    Watch {
        /// Agent/session identifier used for conflict attribution
        #[arg(long)]
        agent: Option<String>,
        /// Debounce window for duplicate file events
        #[arg(long)]
        debounce_ms: Option<u64>,
        /// Conflict window in seconds for cross-agent file contention
        #[arg(long)]
        conflict_window_secs: Option<u64>,
    },

    /// Health checks: index staleness, graph integrity, eval drift
    Doctor {
        /// Attempt to auto-fix detected issues
        #[arg(long)]
        fix: bool,
    },

    /// Upgrade local hief binary and apply safe post-upgrade fixes
    Upgrade,

    /// Git hook management
    #[command(subcommand)]
    Hooks(HooksCmd),

    /// Document scaffolding and template operations
    #[command(subcommand)]
    Docs(DocsCmd),

    /// Skill file management (imperative recipes)
    #[command(subcommand)]
    Skills(SkillsCmd),

    /// MCP server registration for AI coding frameworks
    #[command(subcommand)]
    Mcp(McpCmd),

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
    /// Structural search using ast-grep patterns (e.g., "$FUNC.unwrap()")
    Structural {
        /// ast-grep pattern (use $ for meta-variables: $X.unwrap(), fn $NAME($$$))
        pattern: String,
        /// Programming language (rust, python, typescript)
        #[arg(short, long)]
        language: String,
        /// Maximum number of results
        #[arg(short = 'k', long, default_value = "50")]
        top_k: usize,
    },
    /// Semantic search using vector embeddings (in development)
    Semantic {
        /// Search query
        query: String,
        /// Maximum number of results
        #[arg(short = 'k', long, default_value = "10")]
        top_k: usize,
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
        /// Related skill name to attach to the intent (filename without extension)
        #[arg(long)]
        skill: Option<String>,
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
pub enum PatternsCmd {
    /// List all project-scoped patterns in .hief/patterns/
    List,
    /// Show a pattern's full content
    Show {
        /// Pattern name (e.g. add-api-client)
        name: String,
    },
    /// Create or update a pattern file (reads content from stdin or --content)
    Create {
        /// Pattern name (e.g. add-api-client)
        name: String,
        /// Pattern title (used as # heading if content is empty)
        #[arg(short, long)]
        title: Option<String>,
        /// Inline markdown content (use stdin for longer content)
        #[arg(long)]
        content: Option<String>,
    },
    /// Regenerate .hief/patterns/INDEX.md from files on disk
    Sync,
}

#[derive(Subcommand)]
pub enum HooksCmd {
    /// Install git hooks for auto-indexing, drift checking, and eval
    Install,
    /// Remove HIEF git hooks
    Uninstall,
    /// Show current hook status
    Status,
    /// Alias for Install (install hooks and enable watch mode)
    Watch,
}

#[derive(Subcommand)]
pub enum DocsCmd {
    /// Scaffold docs directory structure (docs/specs/, docs/harness/, .hief/templates/)
    Init,
    /// Generate a document from a template
    Generate {
        /// Template to generate: constitution, spec, data-model, harness, playbook, golden
        template: String,
        /// Name for the feature/scenario/golden set (used in filename and variables)
        #[arg(short, long)]
        name: Option<String>,
        /// Intent ID to link (sets {{id}} variable)
        #[arg(long)]
        id: Option<String>,
        /// Output file path (overrides default)
        #[arg(short, long)]
        output: Option<String>,
        /// Additional variable overrides in KEY=VALUE format (repeatable)
        #[arg(long = "var", value_name = "KEY=VALUE")]
        vars: Vec<String>,
        /// Auto-populate variables from the code index
        #[arg(long)]
        auto_populate: bool,
        /// Overwrite existing file without prompting
        #[arg(long)]
        force: bool,
    },
    /// List available templates
    List,
    /// Check docs directory structure and completeness
    Check,
    /// Show variables for a specific template
    Variables {
        /// Template ID to inspect
        template: String,
    },
}

#[derive(Subcommand)]
pub enum SkillsCmd {
    /// Initialize the skills directory (.hief/skills)
    Init,
    /// List available skill files
    List,
    /// Show the contents of a skill
    Show {
        /// Skill name (filename without extension)
        name: String,
    },
}

#[derive(Subcommand)]
pub enum McpCmd {
    /// Register HIEF as an MCP server in AI coding frameworks (Claude CLI, VS Code, Cursor, etc.)
    Install {
        /// Target framework: claude-cli, claude-desktop, vscode, cursor, windsurf, gemini-cli, or all
        #[arg(default_value = "all")]
        target: String,
        /// Install globally instead of project-level
        #[arg(short, long)]
        global: bool,
    },
    /// Remove HIEF MCP server registration from frameworks
    Uninstall {
        /// Target framework (same options as install)
        #[arg(default_value = "all")]
        target: String,
        /// Uninstall from global config
        #[arg(short, long)]
        global: bool,
    },
    /// Show HIEF MCP registration status in all frameworks
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
