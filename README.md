<div align="center">

# HIEF (Hybrid Intent-Evaluation Framework)

**Persistent Memory Layer for AI Coding Agents**

[![CI](https://github.com/hiranp/hief/actions/workflows/ci.yml/badge.svg)](https://github.com/hiranp/hief/actions/workflows/ci.yml)
[![Crates.io](https://img.shields.io/crates/v/hief.svg)](https://crates.io/crates/hief)
[![Docs.rs](https://docs.rs/hief/badge.svg)](https://docs.rs/hief)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.85%2B-orange.svg)](https://www.rust-lang.org/)
[![MCP](https://img.shields.io/badge/protocol-MCP-green.svg)](https://modelcontextprotocol.io/)

[Getting Started](#getting-started) •
[Features](#features) •
[Documentation](#documentation) •
[Contributing](#contributing) •
[Manifesto](MANIFESTO.md)

</div>

---

AI coding agents are powerful but forgetful. They lose context between sessions,
can't see the full codebase, don't know what other agents are doing, and have no
systematic way to catch quality regressions.

**HIEF** is a local-first [MCP](https://modelcontextprotocol.io/) server that
gives any AI coding agent — Claude Code, Cursor, Copilot, Windsurf, Goose, and
more — **persistent, searchable codebase context**, **lightweight task
coordination**, and **automated quality evaluation**. All from a single Rust
binary. Zero external services. Your code never leaves your machine.

> **HIEF is a sidecar, not an agent.** It has no LLM, doesn't execute code,
> and doesn't make decisions. The host agent provides reasoning; HIEF provides
> memory.

```
Agent (Claude Code / Cursor / Copilot / Windsurf / Goose)
  │
  │  MCP (stdio or HTTP)
  ▼
HIEF Server
  ├── INDEX:   AST-aware search (keyword + structural + semantic)
  ├── INTENTS: Lightweight task coordination + provenance
  └── EVAL:    Golden-set quality guardrails
```

## Features

### 🔍 Code Index — *"What code exists?"*

- **AST-aware chunking** via tree-sitter (Rust, Python, TypeScript/JavaScript)
- **Keyword search** via FTS5 full-text search
- **Structural search** via [ast-grep](https://ast-grep.github.io/) pattern matching (e.g., find every `$X.unwrap()`)
- **Semantic search** via LanceDB vector embeddings *(in development)*
- **Incremental updates** via blake3 content hashing — only re-indexes what changed

### 📋 Intent Graph — *"Who's doing what?"*

- **DAG-based task graph** with status workflow (`draft → in_progress → in_review → approved → done`)
- **Dependency tracking** prevents conflicting concurrent work
- **Provenance records** which agent made each change and when
- Lightweight by design — not a project manager, just enough coordination

### ✅ Quality Evaluation — *"Is the output good enough?"*

- **Golden sets** define project-specific quality criteria in TOML
- **Literal checks** — `must_contain` / `must_not_contain` via substring matching
- **Structural checks** — `structural_must_not_contain` via ast-grep patterns
- **Differential eval** — `diff_only = true` checks only changed files
- **Score history** with regression detection that can block merge

## Getting Started

### Prerequisites

- [Rust](https://rustup.rs/) stable toolchain (MSRV **1.85+**)
- Git (for blame and hooks integration)

### Install from Source

```sh
git clone https://github.com/hiranp/hief.git
cd hief
cargo build --release

# Optional: symlink to PATH for global access
mkdir -p ~/bin
ln -sf "$(pwd)/target/release/hief" ~/bin/hief
```

### Quick Start

```sh
# Initialize HIEF in your project
cd /path/to/your/project
hief init

# Build the code index
hief index build

# Start the MCP server (agents connect here)
hief serve
```

### Connect Your Agent

Configure your AI coding agent to connect to HIEF's MCP server:

**Claude Code / Claude Desktop** — add to your MCP settings:

```json
{
  "mcpServers": {
    "hief": {
      "command": "hief",
      "args": ["serve"]
    }
  }
}
```

**HTTP transport** (for agents that support it):

```sh
hief serve --transport http --port 3100
```

## MCP Tools

HIEF exposes the following tools via the [Model Context Protocol](https://modelcontextprotocol.io/):

| Tool | Purpose |
|------|---------|
| **Search** | |
| `search_code` | Keyword search over indexed code chunks |
| `structural_search` | AST pattern matching (e.g., `$X.unwrap()`) |
| `semantic_search` | Vector similarity search *(in development)* |
| `index_status` | Index statistics (file count, languages, health) |
| `git_blame` | Git authorship info for a file range |
| **Intents** | |
| `create_intent` | Create a task in the dependency graph |
| `list_intents` | List intents filtered by status or kind |
| `update_intent` | Update intent status or assignee |
| `ready_intents` | Show intents whose dependencies are satisfied |
| **Evaluation** | |
| `run_evaluation` | Run golden set quality checks |
| `get_eval_scores` | Score history for a golden set |
| **Context** | |
| `get_project_context` | Index stats + active intents + health overview |
| `get_conventions` | Machine-readable project rules |
| `get_project_health` | Eval scores, regressions, and warnings |

## CLI Reference

```sh
hief init                                    # Initialize HIEF in current project
hief index build                             # Build/rebuild the code index
hief index search "query"                    # Search indexed code
hief index structural '$X.unwrap()' -l rust  # AST pattern search
hief serve                                   # Start MCP server (stdio)
hief serve --transport http --port 3100      # Start MCP server (HTTP)
hief eval run                                # Run golden set evaluation
hief eval report                             # Show eval score history
hief doctor --fix                            # Health check + auto-fix
hief hooks install                           # Install git hooks for auto-indexing
```

## Project Structure

```
.hief/
├── hief.db              # SQLite database (FTS5 index, intents, eval scores)
├── vectors/             # LanceDB directory (semantic embeddings)
└── conventions.toml     # Machine-readable project rules

golden/
└── *.toml               # Golden set evaluation criteria

src/
├── cli/                 # CLI commands (clap)
├── docs/                # Doc scaffolding engine (MiniJinja templates)
├── eval/                # Golden set evaluation engine
├── graph/               # Intent dependency graph (DAG)
├── index/               # AST-aware code indexing + search
│   ├── chunker.rs       # tree-sitter based chunking
│   ├── search.rs        # FTS5 keyword search
│   ├── structural.rs    # ast-grep pattern matching
│   ├── vectors.rs       # LanceDB semantic search
│   └── walker.rs        # File system walker
├── mcp/                 # MCP server (rmcp)
│   ├── tools.rs         # Tool implementations
│   └── resources.rs     # Resource definitions
├── config.rs            # TOML configuration (hief.toml)
├── db.rs                # Database initialization (libsql)
├── errors.rs            # Error types (thiserror)
└── main.rs              # Entry point

hief.toml                # Project-level configuration
templates/               # Doc scaffolding templates
vscode-hief/             # VS Code extension (Kanban, search, dashboard)
```

## What Makes HIEF Different

| Capability | HIEF | Augment Code | Sourcegraph | Cursor |
|------------|------|-------------|-------------|--------|
| Local-first (no cloud) | ✅ | ❌ | ❌ | Partial |
| MCP server (any agent) | ✅ | ❌ | ❌ | ❌ |
| Quality evaluation | ✅ | ❌ | ❌ | ❌ |
| Task coordination | ✅ | ❌ | ❌ | ❌ |
| AST-aware structural search | ✅ | ✅ | ✅ | ✅ |
| Open source | ✅ | ❌ | Partial | ❌ |

**HIEF's unique combination:** local-first + MCP protocol + quality evaluation + open source.

## Documentation

| Document | Description |
|----------|-------------|
| [Manifesto](MANIFESTO.md) | Core beliefs and design philosophy |
| [Constitution](docs/specs/constitution.md) | Inviolable project rules |
| [CHANGELOG](CHANGELOG.md) | Release history |

## VS Code Extension

A companion VS Code extension provides a Kanban board, intent list, dashboard,
and code search view. See [`vscode-hief/`](vscode-hief/) for details.

```sh
cd vscode-hief && npm install && npm run build
```

## Building from Source

```sh
cargo build --release       # Compile release binary
cargo test                  # Run unit tests
cargo fmt -- --check        # Check formatting
cargo clippy --all-targets  # Run linter
```

Or use the [Justfile](Justfile):

```sh
just build                  # cargo build --release
just test                   # cargo test
just lint                   # cargo clippy
just fmt                    # cargo fmt --check
just install                # Build + symlink to ~/bin/hief
```

## Contributing

Contributions are welcome! Please read [CONTRIBUTING.md](CONTRIBUTING.md) for
guidelines on how to get started, the development workflow, and how to submit
pull requests.

## Security

If you discover a security vulnerability, please see [SECURITY.md](SECURITY.md)
for responsible disclosure instructions. **Do not open a public issue.**

## License

HIEF is dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE)
at your option.

## Acknowledgments

HIEF builds on excellent open-source projects:

- [tree-sitter](https://tree-sitter.github.io/) — AST parsing
- [ast-grep](https://ast-grep.github.io/) — Structural search
- [libsql](https://libsql.org/) — Embedded database
- [rmcp](https://github.com/anthropics/rmcp) — MCP server framework
- [LanceDB](https://lancedb.com/) — Vector database
