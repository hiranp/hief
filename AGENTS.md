# AGENTS.md

**Project:** Hybrid Intent‑Evaluation Framework (HIEF)

This repository implements a Rust sidecar that provides local context indexing,
a git‑backed intent graph, and evaluation tooling for AI‑assisted development. The
ultimate goal is to serve as a low‑latency plugin for agents like Copilot, Claude
Code, Windsurf, etc., enabling them to perform safe, auditable modifications in
large, multi‑agent codebases.

## Local MCP Server

Start the HIEF MCP server before doing anything:

```sh
hief serve         # stdio transport (default)
hief serve --transport http --port 3100   # http transport
```

Keep the server running in the background; agents connect here for all tooling operations.

## Available MCP Tools

Agents should utilize these tools for context and task management:

* `search_code`: Search indexed code using FTS5 (text/prefix search).
* `structural_search`: Search code by AST structure using `ast-grep` patterns (e.g., `"$X.unwrap()"`).
* `index_status`: Get statistics on indexed files and languages.
* `create_intent`: Create a new task node in the intent graph.
* `list_intents`: List intents filtered by status or kind.
* `update_intent`: Update intent status (`in_progress`, `in_review`, etc.) or assignee.
* `ready_intents`: Show intents whose dependencies are satisfied.
* `run_evaluation`: Execute golden set evaluations.
* `get_eval_scores`: Retrieve history for a golden set.
* `get_project_context`: High-level overview of index stats and active intents.
* `git_blame`: Get git authorship info for a file range.

## Typical Workflow

### 1. Discovery & Bootstrapping
If no specs exist, use the "Bootstrap Strategy":
1. Run `index_status` to assess project scale.
2. Use `structural_search` and `search_code` to discover architecture and conventions.
3. Run `hief docs init` (via CLI) to scaffold `docs/specs/` and `docs/harness/`.
4. Generate core docs: `hief docs generate constitution`, `hief docs generate data-model`.

### 2. Implementation Loop
1. **Search code**: Use `search_code` and `structural_search` to find relevant logic.
2. **Create intent**: Every change must begin with `create_intent`.
3. **Update status**: Set to `in_progress` when starting work.
4. **Develop & Verify**:
   * Use `structural_search` for quality checks (e.g., finding bare `.unwrap()`).
   * Run evaluations frequently via `run_evaluation`.
5. **Request Review**: Set intent status to `in_review`.

## Best Practices

* **Zero‑latency context.** Rely on the local index tools rather than remote LLM calls for reference.
* **Structural Search vs. Text Search.** Use `structural_search` for finding code by *shape* (e.g., "all public functions with 3 arguments").
* **Small, atomic intents.** Break large features into bite‑sized nodes.
* **Inviolable Rules.** Refer to `docs/specs/constitution.md` for project rules.
* **Harness before merge.** Validate complex behaviors with temporary harnesses (see `docs/harness/`).
* **Human sanity check.** No agent is allowed to mark its own intent `approved`; a human reviewer must intervene.
* **Keep AGENTS.md current.** Add any new tooling commands or workflow conventions here.

## Safety & Identity

* Trust only signed requests to the MCP server.
* Maintain an explicit `assigned_to` field in your intents.
* The local git repo + `.hief/hief.db` is the single source of truth.

## VS Code Extension (optional)

A companion extension (`vscode-hief/`) provides a graphical Kanban board, dashboard, and search results. It is highly recommended for human reviewers.
