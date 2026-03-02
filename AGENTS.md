# AGENTS.md

This project uses **HIEF** (Hybrid Intent‑Evaluation Framework) for agent coordination, indexing, and
evaluation. The document follows the AAIF AGENTS.md conventions and has been augmented with
HIEF‑specific best practices so that any AI coding agent (Copilot, Claude Code, Windsurf, etc.) can
read and act on them.

## Local MCP Server

Start the host agent’s local MCP server before doing anything:

```sh
hief serve         # stdio transport
hief serve --transport http --port 3100   # http transport
```

Keep the server running in the background; agents connect here for all tooling operations.

## Typical Workflow

1. **Search code**  
   Use the `search_code` tool to find definitions, examples, or related logic before touching files.
2. **Create an intent**  
   Every change must begin with `hief intent create …`. The intent is a git‑tracked graph node that
   captures the task description, status, and metadata.
3. **Update intent status**  
   * `in_progress` when you start working.  
   * `in_review` when you believe the work is complete.  
   * `approved`/`rejected` after human review.
4. **Run evaluation**  
   Before merging, run `hief eval run` or `hief eval check` to apply golden‑set criteria and LLM‑judged
   style checks.
5. **Commit & push**  
   All intents, evaluation results, and agent memory live in git; make sure to commit frequently and
   push.

## Best Practices

* **Avoid spec drift.**  Update intents and documentation as you modify code; the graph tracker
  prevents divergence.
* **Zero‑latency context.**  Always rely on the local index (`hief search_code`) rather than remote
  LLM calls for reference.
* **Small, atomic intents.**  Break large features into bite‑sized nodes that can be reviewed
  independently.
* **Harness before merge.**  When adding new behavior, write a temporary harness (`hief harness …`)
  and validate with LATS.
* **Human sanity check.**  No agent is allowed to mark its own intent `approved`; a human reviewer
  must intervene.
* **Keep AGENTS.md current.**  Add any new tooling commands or workflow conventions here so
  other agents can discover them programmatically.
* **.hiefignore.**  Exclude generated artefacts and large binaries from indexing.

## Safety & Identity

* Trust only signed requests to the MCP server.
* Maintain an explicit `agents` section in your intents to record which model or instance made
  changes.
* Treat the local git repo as the single source of truth; replayable agent runs must always start
  from a clean checkout.
