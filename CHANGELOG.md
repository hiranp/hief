# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.3.0] - 2026-05-17

### Added

- **Query Routing & Budgeting**: Smart query routing API separating lexical, semantic, and hybrid search lanes. Search response budgeting with schema-preserving truncation for large fields (keeping file path, symbol metadata, and snippet while bounding token size).
- **Vector Search Cache**: TTL-bounded, schema-validated semantic caching keyed by query fingerprint, embedding hash, and language scope to avoid redundant vector similarity search computations.
- **Observability Subsystem**: Scoped telemetry and event tracking storing search execution metadata (lane, strategy, query metrics) in SQL-native observability tables. Telemetry query integration via SQL VIEWs.
- **Fail-Closed Evaluation Gates**: Robust eval-gate promotion checks. Transitions (`in_review` -> `verified` -> `merged`) are programmatically blocked in the intent pipeline unless the latest evaluation run passes successfully.
- **Multi-Tenant / Multi-Worktree Isolation**: Automatic session partitioning and cognitive memory mapping by Git worktree ID, falling back cleanly to the project-root database for legacy callers.
- **Intent Soft-Locks**: Lease-based exclusive soft locks when intents transition to `in_progress`, rejecting competing worktree ownership via structured, typed conflict errors.
- **Retrieval Weight Learning**: Self-improving retrieval parameters that learn from bounded groundedness windows with shadow-mode scoring, candidate generation, and validation checks.
- **Lexical Overlap Groundedness Scoring**: Deterministic overlap scoring normalized to `[0, 1]` for hybrid retrieval evaluation, removing the dependency on external heavy LLM judges.

### Removed

- **VS Code Extension**: Cleaned up and removed the `vscode-hief` extension subdirectory, files, and companion implementation to fully consolidate HIEF as a fast, language-agnostic headless CLI sidecar and MCP server.

- - -

## [0.2.8] - 2026-04-08

### Added

- MCP tool `run_test_suite` for deterministic local test execution with command selection, timeout controls, and structured pass/fail summaries.
- MCP tool `judge_with_local_model` with pluggable local backends (`ollama` or `custom`) and structured rubric parsing.
- MCP tool `reload_skills` is now explicitly surfaced in the MCP router for hot-reload workflows.

### Changed

- Documentation drift and agent protocol docs updated to reflect expanded MCP surface (drift/context/pattern/routing/test/judge workflows).
- Documentation now aligns with strict verification workflow (`fmt`, `clippy -D warnings`, `test`, guarded eval).

### Fixed

- Clippy `-D warnings` blockers in test-module placement/order (`items_after_test_module`, misplaced test blocks).
- Golden-set `no unwrap` structural guardrails now pass consistently in affected test modules.

- - -

## [0.2.7] - 2026-03-12

### Added

- Refactored skills module and improved code formatting across the project.
- New framework templates for multiple languages and platforms (Next.js, FastAPI, React, Vue, Swift, Kotlin, etc.) to improve LLM onboarding.
- Default HIEF Protocol skill implementation during `hief init`.
- Skills management subsystem allowing creation, listing, and execution of skill recipes. (`skills` module, CLI commands, dynamic MCP tools)
- `hief init` now auto-generates a project-specific `conventions.toml` based on detected languages and dependencies.
- New server tools for recovering stale intents and finding function callers.
- GitHub Actions workflow for automated multi-platform (Linux/macOS/os/Windows) binary releases.

### Changed

- Version bumped to 0.2.7; `Cargo.toml` and `hief.toml` updated accordingly.
- Enhanced conventions auto-generation logic and added options for stale intent recovery and evaluation test commands.
- Documentation paths updated in AGENTS.md; README refined with missing sections and CLI examples; complete docs overhaul for public release.
- Evaluation engine updated with new file pattern checks and test command results.

### Fixed

- Improved security around git command and file-path validation; added new error types and fixed fallback template rendering.

### Security

- Input validation hardened for git commands and path parameters; validated skill names to prevent injection attacks.

## [0.2.6] - 2026-03-12

### Added

- **Skills Engine**: Dynamic MCP tools generated from `.hief/skills/` markdown/YAML files. Any recipe file becomes a callable `execute_skill_<name>` tool automatically.
- **`SkillRegistry`**: In-memory skill registry (`Arc<RwLock<HashMap>>`) with hot-reload support — skills can be refreshed at runtime without restarting the server.
- **`reload_skills` MCP tool**: Hot-reload skill files and return the updated tool list without a server restart.
- **`hief skills` CLI subcommands**: `init`, `list`, and `show` commands for managing skill recipes from the command line.
- **JIT skill injection in `create_intent`**: When a skill is associated with a new intent, the skill content is returned inline in the response, giving the agent executable context immediately.
- **`get_skill` and `list_skills` MCP tools**: Agents can browse and fetch skill content on demand.
- **`get_conventions` MCP tool**: Exposes `.hief/conventions.toml` to agents as a structured tool response.
- **`get_project_health` MCP tool**: Aggregates latest eval scores, regressions, and warnings into a single health snapshot.
- **`get_session_context` MCP tool**: Returns files accessed in the current session and co-access graph suggestions, enabling proactive context loading.
- **`related_files` MCP tool**: Finds files frequently co-accessed with a given file using the Hebbian co-access graph.
- **`structural_search` MCP tool**: Exposes ast-grep pattern matching over the full project via MCP.
- **Pattern cache for structural search**: `OnceLock<Mutex<HashMap>>` avoids re-parsing identical patterns across repeated calls.
- **Multi-platform GitHub Actions release workflow**: Automated cross-platform binary builds and release artifact publishing.

### Changed

- `create_intent` now accepts an optional `skill` parameter and returns `CreateIntentResponse { intent, skill_content }` instead of a bare `Intent`.
- Dynamic `execute_skill_*` wildcard tool routing registered at server startup via `ToolRoute`.
- Structural search results now record accesses into the cognitive memory layer (fire-and-forget, same as `search_code`).
- Documentation overhauled for public release readiness: README, AGENTS.md, CONTRIBUTING.md, SECURITY.md, and all templates.
- MANIFESTO.md moved to project root for improved discoverability.

### Fixed

- Path traversal guard now also rejects paths starting with `-` to prevent command-flag injection in git subprocess calls.
- `validate_top_k` cap (1000) prevents DoS via oversized result requests.
- Broken links in README after documentation restructure.
- Fallback template rendering when MiniJinja encounters unknown variables.

### Security

- Added `HiefError::SecurityViolation` and `HiefError::PathTraversal` variants for explicit security error reporting.
- `validate_path` rejects absolute paths, directory traversal (`..`), and hyphen-prefixed paths in all MCP file-parameter tools (`git_blame`, `related_files`).

## [0.2.5] - 2026-03-05

### Added

- Comprehensive `CONTRIBUTING.md` and `SECURITY.md`.
- `CODE_OF_CONDUCT.md`.
- GitHub Actions CI workflow.
- Issue and Pull Request templates.
- Cognitive memory layer with access tracking and activation-weighted search.
- Vector search capabilities.

### Changed

- Updated `Cargo.toml` with repository metadata.
- Improved version 0.2.5 documentation.
- Enhanced project conventions with structural checks.

### Fixed

- Health-check command to validate golden files.

## [0.2.3] - 2026-03-03

### Added

- `hief upgrade` command for local binary upgrades.
- Literal substring search in scoring engine.
- Golden set specification for project invariants.

### Changed

- Updated documentation templates and enforced mandatory workflow rules.

## [0.2.0] - 2026-02-28

### Added

- Initial public-ready release of HIEF.
- AST-aware indexing for Rust, Python, and TypeScript.
- MCP server implementation with stdio and HTTP transports.
- Intent dependency graph for task coordination.
- Golden set quality evaluation engine.
- VS Code extension companion.
