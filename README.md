# HIEF — Persistent Memory Layer for AI Coding Agents

HIEF is a local-first MCP server that gives AI coding agents **persistent,
searchable codebase context**, **lightweight task coordination**, and
**automated quality evaluation** — all from a single Rust binary with zero
external services.

HIEF is a **sidecar**, not an agent. It does not contain an LLM, does not
execute code, and does not make decisions. The host agent (Claude Code, Cursor,
Copilot, Windsurf, Goose, etc.) provides reasoning. HIEF provides memory.

## Three Capabilities

| Capability | What it solves | Status |
|-----------|---------------|--------|
| **Index** | Agents don't know what code exists | ✅ AST-aware chunking, FTS5 keyword search, ast-grep structural search |
| **Intents** | Multiple agents don't know who's doing what | ✅ DAG-based task graph with status workflow and provenance |
| **Eval** | No systematic way to catch quality regressions | ✅ Golden-set criteria, score history, CI integration |

## Quick Start

```sh
# Install Rust (stable, MSRV 1.85+)
cargo build --release

# Initialize in your project
hief init

# Build the code index
hief index build

# Start the MCP server (agents connect here)
hief serve
```

## How Agents Use HIEF

HIEF exposes MCP tools that any compatible agent can call:

| Tool | Purpose |
|------|---------|
| `search_code` | Keyword search over indexed code chunks |
| `structural_search` | AST pattern matching (e.g., `$X.unwrap()`) |
| `semantic_search` | Vector similarity search *(building)* |
| `get_project_context` | Index stats + active intents + health |
| `get_conventions` | Machine-readable project rules |
| `get_project_health` | Eval scores, regressions, warnings |
| `create_intent` | Create a task in the dependency graph |
| `list_intents` | List intents by status/kind |
| `update_intent` | Update status or assignment |
| `ready_intents` | Show intents ready for work |
| `run_evaluation` | Run golden set quality checks |
| `get_eval_scores` | Score history for a golden set |
| `git_blame` | Git authorship for a file range |

See [Agent Interaction Protocol](docs/agent-protocol.md) for the complete
session lifecycle and search strategy guide.

## Documentation

| Document | Description |
|----------|-------------|
| [Architecture](docs/architecture.md) | How HIEF works — capabilities, storage, MCP interface |
| [Agent Protocol](docs/agent-protocol.md) | When and how agents interact with HIEF |
| [Constitution](docs/specs/constitution.md) | Inviolable project rules |
| [Conventions](/.hief/conventions.toml) | Machine-readable code rules |
| [Development Plan](docs/plan/00-overview.md) | Implementation phases and timeline |

## Project Layout

```
.hief/
├── hief.db              # libsql database (all relational data)
├── vectors/             # LanceDB directory (semantic embeddings)
└── conventions.toml     # Machine-readable project rules

golden/
└── *.toml              # Golden set evaluation criteria

src/
├── index/              # AST-aware code indexing + search
├── graph/              # Intent dependency graph
├── eval/               # Golden set evaluation engine
├── mcp/                # MCP server (tools + resources)
├── cli/                # CLI commands
└── docs/               # Doc scaffolding engine

hief.toml               # Project configuration
```

## Building & Testing

```sh
cargo build --release       # compile release binary
cargo test                  # run unit tests
cargo fmt -- --check        # format check
cargo clippy --all-targets  # lint
```

## Useful Commands

```sh
hief doctor --fix           # health check + auto-fix
hief hooks install          # git hooks for auto-indexing
hief index search "query"   # search indexed code
hief index structural '$X.unwrap()' --language rust
hief eval run               # run golden set evaluation
hief eval report            # show eval score history
```

## VS Code Extension

A companion extension lives in `vscode-hief/`. It provides a Kanban board,
intent list, dashboard, and code search view.

```sh
cd vscode-hief && npm install && npm run build
```

## License

MIT OR Apache-2.0
