# AGENTS.md

**Project:** HIEF — Persistent Memory Layer for AI Coding Agents

HIEF is a local-first MCP server that provides persistent codebase context,
lightweight task coordination, and automated quality evaluation. It is a
**sidecar** — it has no LLM, doesn't execute code, and doesn't make decisions.
The host agent provides reasoning; HIEF provides memory.

## Local MCP Server

Start the HIEF MCP server before doing anything:

```sh
hief serve         # stdio transport (default)
hief serve --transport http --port 3100   # http transport
```

Keep the server running; agents connect here for all tooling operations.

## Available MCP Tools

### Search (Context Retrieval)
* `search_code`: Keyword search over indexed code (FTS5 syntax).
* `structural_search`: AST pattern matching via ast-grep (e.g., `"$X.unwrap()"`).
* `semantic_search`: Vector similarity search — find code by meaning *(building)*.
* `index_status`: Index statistics (file count, languages, health).
* `git_blame`: Git authorship info for a file range.

### Intents (Task Coordination)
* `create_intent`: Create a task in the dependency graph.
* `list_intents`: List intents filtered by status or kind.
* `update_intent`: Update intent status or assignee.
* `ready_intents`: Show intents whose dependencies are satisfied.

### Eval (Quality Guardrails)
* `run_evaluation`: Run golden set evaluation against the codebase.
* `get_eval_scores`: Score history for a golden set.

### Context (Proactive)
* `get_project_context`: High-level overview (index + intents + health).
* `get_conventions`: Machine-readable project rules from `.hief/conventions.toml`.
* `get_project_health`: Eval scores, regressions, and warnings.

## Session Protocol

Follow this protocol for every coding session:

### 1. Orient (Every Session)
```
Call: get_project_context    → Understand project state
Call: get_conventions        → Learn the project's rules
Call: get_project_health     → Check for regressions
```

### 2. Search (Find Context)
```
Know the term?     → search_code "DatabaseConnection"
Know the pattern?  → structural_search "$X.unwrap()" --language rust
Know the concept?  → semantic_search "authentication logic"
Know the file?     → git_blame "src/db.rs" 10 30
```

### 3. Intend (For Non-Trivial Changes)
```
Call: create_intent (kind=feature, title="Add semantic search")
Call: update_intent (status=in_progress, assigned_to=<agent-id>)
```
Skip intents for typos, comment updates, and single-line fixes.

### 4. Execute (Make Changes)
Follow conventions from `.hief/conventions.toml`. Use `structural_search`
to self-check for anti-patterns before submitting.

### 5. Verify (After Changes)
```
Call: run_evaluation         → Check golden set scores
Call: get_eval_scores        → Verify no regressions
Call: update_intent          → Set status to in_review
```

## Conventions

Project rules live in `.hief/conventions.toml` (machine-readable) and
`docs/specs/constitution.md` (human-readable). Key rules:

* All public functions must have doc comments
* No `.unwrap()` in production code paths
* All error types implement `std::error::Error` via thiserror
* No `unsafe` without documented safety invariants
* Async runtime: tokio only
* Every change traces to an intent (for non-trivial changes)
* No agent may approve its own intent
* Golden set regressions block merge

## What HIEF Is NOT

* Not an AI agent — it has no LLM
* Not a project manager — use Jira/Linear for that
* Not a specification framework — it provides conventions, not prose specs
* Not a CI/CD system — it integrates with CI but doesn't replace it

## Safety & Identity

* Maintain an explicit `assigned_to` field in intents.
* The local git repo + `.hief/hief.db` is the single source of truth.
* Intents are optional — the code index works without them.

## Key Documentation

* [Architecture](docs/architecture.md) — How HIEF works
* [Agent Protocol](docs/agent-protocol.md) — Full interaction protocol
* [Constitution](docs/specs/constitution.md) — Inviolable rules
* [Conventions](.hief/conventions.toml) — Machine-readable rules
