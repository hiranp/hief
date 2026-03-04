# HIEF

Hybrid Intent‑Evaluation Framework (HIEF) is a Rust‑based sidecar tool designed
for AI coding agents (e.g. Copilot, Claude Code, Cursor) to provide **zero‑latency local
context**, **git‑backed intent graph tracking**, and **continuous evaluation**
for complex, multi‑agent repositories.

HIEF implements the design outlined in `docs/plan/` and the accompanying
research report (`docs/Strategic Architecture …`). Its high‑level goals are:

1. Provide a local, tree‑sitter/SQLite index of the current working tree.
2. Track agent tasks as git‑versioned graph intents instead of static markdown.
3. Offer a portable MCP server exposing search, intent, and evaluation tools.
4. Support high‑fidelity evaluation (golden sets, LLM judge) before human review.
5. Scaffold SDD/HDD documentation (Constitution, Specs, Harnesses) from existing code.

The architecture follows four sequential phases: **Index → Graph → Eval → Docs → CLI/MCP**
(see `docs/plan/00-overview.md` for details).

## Quick Start

1. Install Rust (stable channel, MSRV 1.85+) and ensure `cargo` is on your PATH.
2. Run `cargo build` to compile the binary.
3. Initialize the project: `hief init`
4. Start the MCP server: `hief serve`

See `AGENTS.md` for workflow and agent instructions.

### Documentation

- `docs/plan/` – implementation plan with phase descriptions
- `docs/Strategic Architecture …` – research report and design rationale
- `docs/sdd-hdd-guide.md` – guide for bootstrapping SDD/HDD on existing codebases

### Golden sets

Golden evaluation cases live under the project root in `.hief/golden/` by default.  Create a new template with:

```sh
hief docs generate golden --name <set-name>
```

The CLI will refuse to emit output until a name is supplied, and doctor will warn if the directory is empty, since no evaluation can run without at least one `.toml` file there.

Existing golden sets may be listed via `hief eval golden list` or with `hief doctor`.



### Building & Testing

```sh
# compile release binary
cargo build --release
```

#### Useful commands

```sh
# project health checker (auto‑fix with --fix)
hief doctor --fix

# manage git hooks for auto‑indexing and eval gating
hief hooks install
hief hooks status

# search indexed code
hief index search "query"
hief index structural "$X.unwrap()" --language rust
```

```sh
# format & lint
cargo fmt -- --check
cargo clippy --all-targets --all-features -- -D warnings

# run unit tests
cargo test
```

Pull requests should follow the contributing guidelines in `CONTRIBUTING.md`.

### VS Code Extension

A companion Visual Studio Code extension lives in the `vscode-hief/` directory. It
provides a Kanban board, intent list panel, dashboard (including doctor status),
code search view, and hooks evaluation. Build it using:

```sh
cd vscode-hief
npm install
npm run build
```

The extension communicates with the `hief` CLI via `--json` output; TypeScript
types in `src/backend/types.ts` mirror the Rust data structures.
