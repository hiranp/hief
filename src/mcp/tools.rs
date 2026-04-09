//! MCP tool definitions for the HIEF server.

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use rmcp::handler::server::tool::ToolRouter;
use rmcp::handler::server::wrapper::{Json, Parameters};
use rmcp::model::{Implementation, ServerCapabilities, ServerInfo};
use rmcp::schemars;
use rmcp::{ErrorData, ServerHandler, tool, tool_handler, tool_router};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tokio::io::AsyncReadExt;

use super::resources;
use super::skills::SkillRegistry;
use crate::config::Config;
use crate::db::Database;
use crate::graph;
use crate::graph::edges::IntentEdge;
use crate::graph::intent::Intent;
use crate::index;
use crate::index::search::{SearchQuery, SearchResult};
use crate::index::structural;
use rmcp::handler::server::tool::ToolName;

/// The HIEF MCP server handler.
#[derive(Clone)]
pub struct HiefServer {
    db: Database,
    project_root: PathBuf,
    /// Cached project config. Loaded once at startup to avoid per-request disk I/O.
    /// Use `hief serve` restart to pick up `hief.toml` changes.
    config: Config,
    tool_router: ToolRouter<Self>,
    skills: SkillRegistry,
}

impl HiefServer {
    /// Construct a new server instance and register dynamic skill tools.
    pub fn new(db: Database, project_root: PathBuf) -> Self {
        let mut tool_router = Self::tool_router();

        // Load config once at startup; fall back to safe defaults if hief.toml is absent.
        let config = Config::load(&project_root.join("hief.toml")).unwrap_or_default();

        // load dynamic skills and register their tool definitions
        let skills = SkillRegistry::new();
        if let Err(e) = skills.load_from_disk(&project_root) {
            tracing::warn!("failed to load skills: {}", e);
        }
        // register each skill using the generic handler; ToolRoute ensures both
        // schema and handler are wired up.
        for tool in skills.tool_defs() {
            let route = rmcp::handler::server::router::tool::ToolRoute::new(
                tool.clone(),
                HiefServer::execute_dynamic_skill,
            );
            tool_router = tool_router.with_route(route);
        }

        Self {
            db,
            project_root,
            config,
            tool_router,
            skills,
        }
    }

    /// Validates that a path is relative to the project root and doesn't escape it.
    fn validate_path(&self, path: &str) -> std::result::Result<PathBuf, ErrorData> {
        let p = std::path::Path::new(path);
        if p.is_absolute() || path.contains("..") {
            let err = crate::errors::HiefError::PathTraversal(path.to_string());
            return Err(ErrorData::invalid_params(err.to_string(), None));
        }

        // Ensure path doesn't start with a hyphen (command flag injection protection)
        if path.starts_with('-') {
            let err = crate::errors::HiefError::SecurityViolation(format!(
                "Invalid path '{}': cannot start with a hyphen",
                path
            ));
            return Err(ErrorData::invalid_params(err.to_string(), None));
        }

        let full_path = self.project_root.join(p);
        Ok(full_path)
    }

    /// Validates and limits top_k to prevent DoS.
    fn validate_top_k(&self, top_k: Option<usize>, default: usize) -> usize {
        let val = top_k.unwrap_or(default);
        if val > 1000 { 1000 } else { val }
    }
}

// -- Parameter structs --

/// Wrapper useful for tool outputs: ensures root type is `object` in schema.
#[derive(Serialize, JsonSchema)]
pub struct ObjectResponse<T> {
    pub result: T,
}

#[derive(Serialize, JsonSchema)]
pub struct SemanticSearchResponse {
    pub status: String,
    pub message: Option<String>,
    pub query: String,
    pub top_k: usize,
    pub language: Option<String>,
    pub results: Vec<crate::index::vectors::SemanticResult>,
}

#[derive(Serialize, JsonSchema)]
pub struct ProjectContext {
    pub index: crate::index::IndexStats,
    pub active_intents: Vec<crate::graph::intent::Intent>,
    pub ready_intents: Vec<crate::graph::intent::Intent>,
}

#[derive(Serialize, JsonSchema)]
pub struct CreateIntentResponse {
    pub intent: Intent,
    pub skill_content: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema, Default)]
pub struct SearchCodeParams {
    /// Search query (FTS5 syntax)
    pub query: String,
    /// Max results to return (default: 10)
    pub top_k: Option<usize>,
    /// Filter by programming language
    pub language: Option<String>,
    /// Filter by symbol kind
    pub symbol_kind: Option<String>,
    /// Boost results by access history (cognitive memory). Default: false.
    pub boost_by_history: Option<bool>,
    /// MCP session identifier for access tracking
    pub session_id: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema, Default)]
pub struct CreateIntentParams {
    /// Kind: feature, bug, refactor, spike, test, chore
    pub kind: String,
    /// Short descriptive title
    pub title: String,
    /// Detailed description
    pub description: Option<String>,
    /// Priority: critical, high, medium, low
    pub priority: Option<String>,
    /// Comma-separated intent IDs this depends on
    pub depends_on: Option<String>,
    /// Optional skill name to associate with the intent (filename without extension)
    pub skill: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema, Default)]
pub struct ListIntentsParams {
    /// Filter by status
    pub status: Option<String>,
    /// Filter by kind
    pub kind: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema, Default)]
pub struct UpdateIntentParams {
    /// Intent UUID
    pub id: String,
    /// New status
    pub status: Option<String>,
    /// Assign to agent ID or human
    pub assigned_to: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema, Default)]
pub struct RunEvaluationParams {
    /// Specific golden set name, or omit for all
    pub golden_set: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema, Default)]
pub struct GetEvalScoresParams {
    /// Golden set name
    pub golden_set: String,
    /// Number of recent entries (default: 10)
    pub limit: Option<usize>,
}

#[derive(Deserialize, schemars::JsonSchema, Default)]
pub struct GitBlameParams {
    /// File path relative to project root
    pub file: String,
    /// Start line (0-indexed)
    pub start_line: u32,
    /// End line (0-indexed)
    pub end_line: u32,
}

#[derive(Deserialize, schemars::JsonSchema, Default)]
pub struct StructuralSearchParams {
    /// ast-grep pattern (e.g., "$X.method()", "fn $NAME($$$) { $$$BODY }")
    pub pattern: String,
    /// Programming language: rust, python, typescript, javascript
    pub language: String,
    /// Max results to return (default: 50)
    pub top_k: Option<usize>,
    /// MCP session identifier for access tracking
    pub session_id: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema, Default)]
pub struct RelatedFilesParams {
    /// File path to find related files for
    pub file: String,
    /// Max results to return (default: 10)
    pub top_k: Option<usize>,
}

#[derive(Serialize, JsonSchema)]
pub struct ReloadSkillsResponse {
    /// Number of skills now loaded in the registry.
    pub count: usize,
    /// Tool names for all loaded skills (e.g. `execute_skill_foo`).
    pub skill_names: Vec<String>,
}

#[derive(Deserialize, schemars::JsonSchema, Default)]
pub struct GetSkillParams {
    /// Skill name (filename without extension)
    pub name: String,
}

#[derive(Deserialize, schemars::JsonSchema, Default)]
pub struct GetSessionContextParams {
    /// MCP session identifier
    pub session_id: String,
    /// Max related file suggestions (default: 10)
    pub suggestion_limit: Option<usize>,
}

#[derive(Deserialize, schemars::JsonSchema, Default)]
pub struct SemanticSearchParams {
    /// Natural language query (e.g., "authentication and authorization logic")
    pub query: String,
    /// Max results to return (default: 10)
    pub top_k: Option<usize>,
    /// Filter by programming language
    pub language: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema, Default)]
pub struct GetTransitiveDepsParams {
    /// Intent ID (full or short prefix)
    pub id: String,
}

#[derive(Deserialize, schemars::JsonSchema, Default)]
pub struct FindCallersParams {
    /// Name of the function to find call sites for
    pub function_name: String,
    /// Programming language: rust, python, typescript, javascript
    pub language: String,
    /// Max results to return (default: 50)
    pub top_k: Option<usize>,
}

#[derive(Deserialize, schemars::JsonSchema, Default)]
pub struct ReadContextFileParams {
    /// Context file name (with or without .md extension, e.g. "architecture")
    pub name: String,
}

#[derive(Deserialize, schemars::JsonSchema, Default)]
pub struct WriteContextFileParams {
    /// Context file name (without path, e.g. "architecture" or "my-notes")
    pub name: String,
    /// Markdown content to write
    pub content: String,
}

#[derive(Deserialize, schemars::JsonSchema, Default)]
pub struct GetPatternParams {
    /// Pattern name (without .md extension)
    pub name: String,
}

#[derive(Deserialize, schemars::JsonSchema, Default)]
pub struct CreatePatternParams {
    /// Pattern name — alphanumerics, hyphens, underscores (e.g. "add-api-client")
    pub name: String,
    /// One-line title describing the pattern
    pub title: Option<String>,
    /// Full markdown content. If omitted, a stub template is created.
    pub content: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema, Default)]
pub struct RunTestSuiteParams {
    /// Command to execute (shell string). Defaults to `cargo test --all-features`.
    pub command: Option<String>,
    /// Timeout in seconds. Defaults to 600.
    pub timeout_secs: Option<u64>,
    /// Optional working directory relative to project root.
    pub working_dir: Option<String>,
}

#[derive(Serialize, JsonSchema)]
pub struct RunTestSuiteResponse {
    pub command: String,
    pub working_dir: String,
    pub timeout_secs: u64,
    pub timed_out: bool,
    pub passed: bool,
    pub exit_code: Option<i32>,
    pub summary_lines: Vec<String>,
    pub stdout_tail: Vec<String>,
    pub stderr_tail: Vec<String>,
}

#[derive(Deserialize, schemars::JsonSchema, Default)]
pub struct JudgeWithLocalModelParams {
    /// The candidate output, patch summary, or artifact to evaluate.
    pub prompt: String,
    /// Optional rubric to apply during evaluation.
    pub rubric: Option<String>,
    /// Backend to run: `ollama` or `custom`.
    pub backend: Option<String>,
    /// Model name for ollama backend. Defaults to `llama3.1:8b`.
    pub model: Option<String>,
    /// Custom shell command for backend=custom.
    pub command: Option<String>,
    /// Timeout in seconds. Defaults to 120.
    pub timeout_secs: Option<u64>,
}

#[derive(Serialize, JsonSchema)]
pub struct JudgeParsed {
    pub score_0_to_100: Option<f64>,
    pub verdict: Option<String>,
    pub rationale: Option<String>,
    pub risks: Vec<String>,
    pub suggestions: Vec<String>,
}

#[derive(Serialize, JsonSchema)]
pub struct JudgeWithLocalModelResponse {
    pub backend: String,
    pub model: Option<String>,
    pub command: String,
    pub timeout_secs: u64,
    pub timed_out: bool,
    pub exit_code: Option<i32>,
    pub parsed: Option<JudgeParsed>,
    pub raw_output: String,
    pub stderr_tail: Vec<String>,
}

#[derive(Serialize, JsonSchema)]
pub struct FindCallersResponse {
    pub function_name: String,
    pub language: String,
    pub callers: Vec<crate::index::structural::StructuralMatch>,
    pub count: usize,
}

#[tool_router]
impl HiefServer {
    #[tool(
        name = "search_code",
        description = "Search indexed code with optional cognitive memory boost. Returns matching chunks with file paths, symbol names, and snippets. Supports FTS5 syntax: prefix*, phrases, AND/OR, column:filter. Set boost_by_history=true to rank recently/frequently accessed code higher."
    )]
    async fn search_code(
        &self,
        Parameters(params): Parameters<SearchCodeParams>,
    ) -> Result<Json<ObjectResponse<Vec<SearchResult>>>, ErrorData> {
        let mut search_query = SearchQuery::new(&params.query);
        search_query.top_k = self.validate_top_k(params.top_k, 10);
        search_query.language = params.language;
        search_query.symbol_kind = params.symbol_kind;

        let mut results = index::search(&self.db, &search_query)
            .await
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;

        // Step 4: Apply activation-weighted boost if requested
        if params.boost_by_history.unwrap_or(false) && !results.is_empty() {
            let file_paths: Vec<String> = results.iter().map(|r| r.file_path.clone()).collect();
            let boosts = index::memory::batch_access_boost(&self.db, &file_paths)
                .await
                .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;

            for result in &mut results {
                if let Some(&boost) = boosts.get(&result.file_path) {
                    // relevance_score = fts5_rank * (1.0 + access_boost)
                    result.rank *= 1.0 + boost;
                }
            }

            // Re-sort by boosted rank (FTS5 rank is negative, more negative = better)
            results.sort_by(|a, b| {
                a.rank
                    .partial_cmp(&b.rank)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
        }

        // Step 2: Record access for cognitive memory (fire-and-forget, don't block response)
        let file_paths: Vec<String> = results.iter().map(|r| r.file_path.clone()).collect();
        if !file_paths.is_empty() {
            let db = self.db.clone();
            let query_str = params.query.clone();
            let session = params.session_id.clone();
            tokio::spawn(async move {
                let _ = index::memory::record_search_accesses(
                    &db,
                    &file_paths,
                    Some(&query_str),
                    "search_code",
                    session.as_deref(),
                )
                .await;
            });
        }

        Ok(Json(ObjectResponse { result: results }))
    }

    #[tool(
        name = "index_status",
        description = "Get the current index statistics: file count, chunk count, languages, last indexed time, and database size."
    )]
    async fn index_status(&self) -> Result<Json<ObjectResponse<index::IndexStats>>, ErrorData> {
        let stats = index::status(&self.db, &self.project_root)
            .await
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;

        Ok(Json(ObjectResponse { result: stats }))
    }

    #[tool(
        name = "create_intent",
        description = "Create a new intent (task) in the dependency graph. Returns the created intent with its UUID."
    )]
    async fn create_intent(
        &self,
        Parameters(params): Parameters<CreateIntentParams>,
    ) -> Result<Json<CreateIntentResponse>, ErrorData> {
        let mut intent = Intent::new(
            params.kind,
            params.title,
            params.description,
            params.priority,
        );

        // record skill name as a label if provided
        if let Some(skill_name) = &params.skill {
            intent.labels.push(format!("skill:{}", skill_name));
        }

        graph::create_intent(&self.db, &intent)
            .await
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;

        if let Some(deps) = params.depends_on {
            for dep_id in deps.split(',').map(|s| s.trim()) {
                if !dep_id.is_empty() {
                    let edge = IntentEdge::depends_on(&intent.id, dep_id);
                    graph::add_edge(&self.db, &edge)
                        .await
                        .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
                }
            }
        }

        // build response object with optional skill content
        let mut skill_content = None;
        if let Some(skill_name) = &params.skill {
            let skills_dir = self.project_root.join(&self.config.skills.skills_path);
            let candidates = ["md", "yaml", "yml", "txt"];
            for ext in &candidates {
                let path = skills_dir.join(format!("{}.{}", skill_name, ext));
                if path.exists() {
                    if let Ok(txt) = std::fs::read_to_string(&path) {
                        skill_content = Some(txt);
                    }
                    break;
                }
            }
        }

        Ok(Json(CreateIntentResponse {
            intent,
            skill_content,
        }))
    }

    #[tool(
        name = "list_intents",
        description = "List intents in the dependency graph, optionally filtered by status and/or kind."
    )]
    async fn list_intents(
        &self,
        Parameters(params): Parameters<ListIntentsParams>,
    ) -> Result<Json<ObjectResponse<Vec<Intent>>>, ErrorData> {
        let intents =
            graph::list_intents(&self.db, params.status.as_deref(), params.kind.as_deref())
                .await
                .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;

        Ok(Json(ObjectResponse { result: intents }))
    }

    #[tool(
        name = "update_intent",
        description = "Update an intent status or assignment. Status transitions are validated."
    )]
    async fn update_intent(
        &self,
        Parameters(params): Parameters<UpdateIntentParams>,
    ) -> Result<Json<Intent>, ErrorData> {
        if let Some(new_status) = &params.status {
            graph::update_status(&self.db, &params.id, new_status)
                .await
                .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
        }

        if let Some(assignee) = &params.assigned_to {
            graph::assign_intent(&self.db, &params.id, assignee)
                .await
                .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
        }

        let intent = graph::get_intent(&self.db, &params.id)
            .await
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;

        Ok(Json(intent))
    }

    #[tool(
        name = "ready_intents",
        description = "Show intents that are approved and whose all dependencies are satisfied."
    )]
    async fn ready_intents(&self) -> Result<Json<ObjectResponse<Vec<Intent>>>, ErrorData> {
        let intents = graph::ready_intents(&self.db)
            .await
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;

        Ok(Json(ObjectResponse { result: intents }))
    }

    #[tool(
        name = "run_evaluation",
        description = "Run golden set evaluation against the indexed codebase."
    )]
    async fn run_evaluation(
        &self,
        Parameters(params): Parameters<RunEvaluationParams>,
    ) -> Result<Json<ObjectResponse<Vec<crate::eval::EvalResult>>>, ErrorData> {
        let results = crate::eval::run(
            &self.db,
            &self.project_root,
            &self.config.eval,
            params.golden_set.as_deref(),
        )
        .await
        .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;

        Ok(Json(ObjectResponse { result: results }))
    }

    #[tool(
        name = "run_test_suite",
        description = "Run a local test command with timeout and return structured pass/fail output. Defaults to 'cargo test --all-features'."
    )]
    async fn run_test_suite(
        &self,
        Parameters(params): Parameters<RunTestSuiteParams>,
    ) -> Result<Json<ObjectResponse<RunTestSuiteResponse>>, ErrorData> {
        let command = params
            .command
            .unwrap_or_else(|| "cargo test --all-features".to_string());
        let timeout_secs = params.timeout_secs.unwrap_or(600).clamp(1, 3600);

        let working_dir = match params.working_dir {
            Some(dir) => self.validate_path(&dir)?,
            None => self.project_root.clone(),
        };

        let outcome = run_shell_command(&command, &working_dir, timeout_secs)
            .await
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;

        let combined = format!("{}\n{}", outcome.stdout, outcome.stderr);
        let summary_lines = collect_summary_lines(&combined);
        let response = RunTestSuiteResponse {
            command,
            working_dir: working_dir.display().to_string(),
            timeout_secs,
            timed_out: outcome.timed_out,
            passed: !outcome.timed_out && outcome.exit_code == Some(0),
            exit_code: outcome.exit_code,
            summary_lines,
            stdout_tail: tail_lines(&outcome.stdout, 80),
            stderr_tail: tail_lines(&outcome.stderr, 80),
        };

        Ok(Json(ObjectResponse { result: response }))
    }

    #[tool(
        name = "judge_with_local_model",
        description = "Run a local judge backend (ollama or custom) and return structured rubric scoring plus raw output."
    )]
    async fn judge_with_local_model(
        &self,
        Parameters(params): Parameters<JudgeWithLocalModelParams>,
    ) -> Result<Json<ObjectResponse<JudgeWithLocalModelResponse>>, ErrorData> {
        if params.prompt.trim().is_empty() {
            return Err(ErrorData::invalid_params(
                "prompt must not be empty".to_string(),
                None,
            ));
        }

        let backend = params
            .backend
            .unwrap_or_else(|| "ollama".to_string())
            .to_lowercase();
        let timeout_secs = params.timeout_secs.unwrap_or(120).clamp(1, 1800);

        let rubric = params.rubric.unwrap_or_else(|| {
            "Score from 0-100. Evaluate correctness, risk, completeness, and testability."
                .to_string()
        });

        let judge_prompt = build_judge_prompt(&rubric, &params.prompt);

        let (command_label, model, outcome) = match backend.as_str() {
            "ollama" => {
                let model = params.model.unwrap_or_else(|| "llama3.1:8b".to_string());
                let command_label = format!("ollama run {} <judge-prompt>", model);
                let outcome = run_command_with_input(
                    "ollama",
                    &["run", &model],
                    Some(&judge_prompt),
                    &self.project_root,
                    timeout_secs,
                    &[],
                )
                .await
                .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
                (command_label, Some(model), outcome)
            }
            "custom" => {
                let custom = params.command.ok_or_else(|| {
                    ErrorData::invalid_params(
                        "command is required when backend=custom".to_string(),
                        None,
                    )
                })?;
                let outcome = run_command_with_input(
                    "sh",
                    &["-c", &custom],
                    None,
                    &self.project_root,
                    timeout_secs,
                    &[
                        ("HIEF_JUDGE_PROMPT", judge_prompt.as_str()),
                        ("HIEF_JUDGE_RUBRIC", rubric.as_str()),
                        ("HIEF_JUDGE_INPUT", params.prompt.as_str()),
                    ],
                )
                .await
                .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
                (custom, None, outcome)
            }
            other => {
                return Err(ErrorData::invalid_params(
                    format!("unsupported backend '{}'; use ollama|custom", other),
                    None,
                ));
            }
        };

        let parsed = parse_judge_output(&outcome.stdout);
        let response = JudgeWithLocalModelResponse {
            backend,
            model,
            command: command_label,
            timeout_secs,
            timed_out: outcome.timed_out,
            exit_code: outcome.exit_code,
            parsed,
            raw_output: outcome.stdout,
            stderr_tail: tail_lines(&outcome.stderr, 80),
        };

        Ok(Json(ObjectResponse { result: response }))
    }

    #[tool(
        name = "get_eval_scores",
        description = "Get evaluation score history for a golden set."
    )]
    async fn get_eval_scores(
        &self,
        Parameters(params): Parameters<GetEvalScoresParams>,
    ) -> Result<Json<ObjectResponse<Vec<crate::eval::history::ScoreEntry>>>, ErrorData> {
        let history = crate::eval::history::get_history(
            &self.db,
            &params.golden_set,
            params.limit.unwrap_or(10),
        )
        .await
        .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;

        Ok(Json(ObjectResponse { result: history }))
    }

    #[tool(
        name = "get_project_context",
        description = "Get a high-level project overview: index stats, active intents, and ready intents."
    )]
    async fn get_project_context(&self) -> Result<Json<ObjectResponse<ProjectContext>>, ErrorData> {
        let stats = index::status(&self.db, &self.project_root)
            .await
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;

        let active_intents = graph::list_intents(&self.db, Some("in_progress"), None)
            .await
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;

        let ready = graph::ready_intents(&self.db)
            .await
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;

        let context = ProjectContext {
            index: stats,
            active_intents,
            ready_intents: ready,
        };
        Ok(Json(ObjectResponse { result: context }))
    }

    #[tool(
        name = "git_blame",
        description = "Get git blame information for a file range."
    )]
    async fn git_blame(
        &self,
        Parameters(params): Parameters<GitBlameParams>,
    ) -> Result<Json<ObjectResponse<String>>, ErrorData> {
        let _ = self.validate_path(&params.file)?;
        let result =
            crate::index::search::git_blame_range(&params.file, params.start_line, params.end_line)
                .await
                .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
        Ok(Json(ObjectResponse { result }))
    }

    #[tool(
        name = "structural_search",
        description = "Search code by AST structure using ast-grep patterns. Finds code matching structural patterns like '$X.method()', 'fn $NAME($$$) { $$$BODY }', or 'if let Err($E) = $EXPR { $$$BODY }'. Use $ for single-node meta-variables and $$$ for variadic (multi-node) meta-variables."
    )]
    async fn structural_search(
        &self,
        Parameters(params): Parameters<StructuralSearchParams>,
    ) -> Result<Json<ObjectResponse<Vec<crate::index::structural::StructuralMatch>>>, ErrorData>
    {
        let mut query = structural::StructuralQuery::new(&params.pattern, &params.language);
        query.top_k = self.validate_top_k(params.top_k, 50);

        let results = structural::search(&self.project_root, &query)
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;

        // Record access for cognitive memory (fire-and-forget)
        let file_paths: Vec<String> = results.iter().map(|r| r.file_path.clone()).collect();
        if !file_paths.is_empty() {
            let db = self.db.clone();
            let pattern = params.pattern.clone();
            let session = params.session_id.clone();
            tokio::spawn(async move {
                let _ = index::memory::record_search_accesses(
                    &db,
                    &file_paths,
                    Some(&pattern),
                    "structural_search",
                    session.as_deref(),
                )
                .await;
            });
        }

        Ok(Json(ObjectResponse { result: results }))
    }

    #[tool(
        name = "related_files",
        description = "Find files related to a given file using the cognitive co-access graph. Returns files that are frequently accessed together with the input file, ranked by co-access strength. Useful for discovering related code without searching."
    )]
    async fn related_files(
        &self,
        Parameters(params): Parameters<RelatedFilesParams>,
    ) -> Result<Json<ObjectResponse<Vec<crate::index::memory::RelatedFile>>>, ErrorData> {
        let _ = self.validate_path(&params.file)?;
        let top_k = self.validate_top_k(params.top_k, 10);

        let related = index::memory::related_files(&self.db, &params.file, top_k)
            .await
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;

        Ok(Json(ObjectResponse { result: related }))
    }

    #[tool(
        name = "list_skills",
        description = "List all skill filenames available under the project's skills directory (usually .hief/skills)."
    )]
    async fn list_skills(&self) -> Result<Json<ObjectResponse<Vec<String>>>, ErrorData> {
        let skills_dir = self.project_root.join(&self.config.skills.skills_path);
        let mut names = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&skills_dir) {
            for entry in entries.flatten() {
                if let Some(stem) = entry.path().file_stem().and_then(|s| s.to_str()) {
                    if stem != "README" {
                        names.push(stem.to_string());
                    }
                }
            }
        }
        Ok(Json(ObjectResponse { result: names }))
    }

    #[tool(
        name = "get_skill",
        description = "Fetch the text contents of a named skill file. Supply the skill name without extension."
    )]
    async fn get_skill(
        &self,
        Parameters(params): Parameters<GetSkillParams>,
    ) -> Result<Json<ObjectResponse<String>>, ErrorData> {
        let skills_dir = self.project_root.join(&self.config.skills.skills_path);
        let candidates = ["md", "yaml", "yml", "txt"];
        for ext in &candidates {
            let path = skills_dir.join(format!("{}.{}", params.name, ext));
            if path.exists() {
                let contents = std::fs::read_to_string(&path)
                    .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
                return Ok(Json(ObjectResponse { result: contents }));
            }
        }
        Err(ErrorData::invalid_params(
            format!("skill not found: {}", params.name),
            None,
        ))
    }

    #[tool(
        name = "get_session_context",
        description = "Get session-aware context: files accessed this session (with counts), related files not yet accessed (from co-access graph). Provides proactive context so the agent knows what it has looked at and what else might be relevant."
    )]
    async fn get_session_context(
        &self,
        Parameters(params): Parameters<GetSessionContextParams>,
    ) -> Result<Json<ObjectResponse<crate::index::memory::SessionContext>>, ErrorData> {
        let limit = self.validate_top_k(params.suggestion_limit, 10);

        let ctx = index::memory::get_session_context(&self.db, &params.session_id, limit)
            .await
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;

        Ok(Json(ObjectResponse { result: ctx }))
    }

    #[tool(
        name = "semantic_search",
        description = "Search code by meaning using vector similarity. Find code related to a concept even when exact keywords don't appear in the source. Requires vector index to be enabled and built. Returns matching chunks ranked by semantic similarity."
    )]
    async fn semantic_search(
        &self,
        Parameters(params): Parameters<SemanticSearchParams>,
    ) -> Result<Json<SemanticSearchResponse>, ErrorData> {
        if !self.config.vectors.enabled {
            return Err(ErrorData::internal_error(
                "Semantic search is not enabled. Set vectors.enabled = true in hief.toml and rebuild the index.".to_string(),
                None,
            ));
        }

        let query_vector =
            crate::index::vectors::embed_text(&params.query, self.config.vectors.dimensions)
                .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
        let query = crate::index::vectors::SemanticQuery {
            query: params.query.clone(),
            top_k: self.validate_top_k(params.top_k, 10),
            language: params.language.clone(),
        };
        let results = crate::index::vectors::search(
            &self.project_root,
            &query_vector,
            &query,
            &self.config.vectors,
        )
        .await
        .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;

        let resp = SemanticSearchResponse {
            status: "ok".to_string(),
            message: if results.is_empty() {
                Some("No semantic matches found for the query.".to_string())
            } else {
                None
            },
            query: params.query.clone(),
            top_k: query.top_k,
            language: params.language.clone(),
            results,
        };

        Ok(Json(resp))
    }

    #[tool(
        name = "get_conventions",
        description = "Get the project's machine-readable conventions from .hief/conventions.toml. Returns rules that the agent should follow when writing code, including check patterns, scopes, and severity levels."
    )]
    async fn get_conventions(
        &self,
    ) -> Result<Json<ObjectResponse<resources::ProjectConventions>>, ErrorData> {
        let conventions = resources::get_project_conventions(&self.project_root)
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;

        Ok(Json(ObjectResponse {
            result: conventions,
        }))
    }

    #[tool(
        name = "get_project_health",
        description = "Get project health: latest eval scores, regressions, and warnings. Use this to check if the codebase is in good shape before starting work."
    )]
    async fn get_project_health(
        &self,
    ) -> Result<Json<ObjectResponse<resources::ProjectHealth>>, ErrorData> {
        let health = resources::get_project_health(&self.db, &self.project_root, &self.config)
            .await
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;

        Ok(Json(ObjectResponse { result: health }))
    }

    // ------------------------------------------------------------------
    // Skills management
    // ------------------------------------------------------------------

    #[tool(
        name = "recover_stale_intents",
        description = "Recover intents stuck in in_progress beyond the stale timeout configured \
                       in hief.toml (graph.stale_timeout_hours, default 48 h). Resets them to \
                       approved so another agent can pick them up. This is the deadlock escape \
                       hatch: if an agent crashes or times out while holding an intent, call \
                       this tool to unblock the graph. Returns the count of recovered intents."
    )]
    async fn recover_stale_intents(&self) -> Result<Json<ObjectResponse<usize>>, ErrorData> {
        let timeout_hours = self.config.graph.stale_timeout_hours;
        let count = graph::recover_stale_intents(&self.db, timeout_hours)
            .await
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
        Ok(Json(ObjectResponse { result: count }))
    }

    #[tool(
        name = "get_transitive_deps",
        description = "Get all transitive (recursive) dependencies of an intent. Returns every \
                       intent that must reach verified or merged status before this one can \
                       proceed. Useful for understanding the full dependency chain before \
                       starting work on a deeply nested task."
    )]
    async fn get_transitive_deps(
        &self,
        Parameters(params): Parameters<GetTransitiveDepsParams>,
    ) -> Result<Json<ObjectResponse<Vec<Intent>>>, ErrorData> {
        let id = graph::resolve_id(&self.db, &params.id)
            .await
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
        let deps = graph::transitive_deps(&self.db, &id)
            .await
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
        Ok(Json(ObjectResponse { result: deps }))
    }

    #[tool(
        name = "find_callers",
        description = "Find all call sites of a named function using deterministic AST pattern \
                       matching. Searches for both free-function calls (foo($$$)) and method \
                       calls (recv.foo($$$)). This is a Code Property Graph traversal — far \
                       more reliable than vector similarity for finding references. Useful for \
                       impact analysis: 'what breaks if I change verify_password()?'"
    )]
    async fn find_callers(
        &self,
        Parameters(params): Parameters<FindCallersParams>,
    ) -> Result<Json<FindCallersResponse>, ErrorData> {
        let top_k = self.validate_top_k(params.top_k, 50);

        // Pattern for free-function call: fn_name($$$)
        let free_pattern = format!("{}($$$)", params.function_name);
        let mut query = structural::StructuralQuery::new(&free_pattern, &params.language);
        query.top_k = top_k;
        let mut callers = structural::search(&self.project_root, &query)
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;

        // Pattern for method call: recv.fn_name($$$)
        let method_pattern = format!("$RECV.{}($$$)", params.function_name);
        let mut mq = structural::StructuralQuery::new(&method_pattern, &params.language);
        mq.top_k = top_k;
        let method_callers = structural::search(&self.project_root, &mq)
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
        callers.extend(method_callers);

        // Deduplicate by (file, line) and enforce top_k cap
        callers.sort_by(|a, b| {
            a.file_path
                .cmp(&b.file_path)
                .then(a.start_line.cmp(&b.start_line))
        });
        callers.dedup_by(|a, b| a.file_path == b.file_path && a.start_line == b.start_line);
        callers.truncate(top_k);

        let count = callers.len();
        Ok(Json(FindCallersResponse {
            function_name: params.function_name,
            language: params.language,
            callers,
            count,
        }))
    }

    // ------------------------------------------------------------------
    // Drift detection
    // ------------------------------------------------------------------

    #[tool(
        name = "check_drift",
        description = "Run all documentation drift checkers and return a 0-100 score with issues. Score 100 = perfectly in sync; deductions for missing context files (-10), stale files (-3), and missing patterns index (-1). CI-safe: no side effects."
    )]
    async fn check_drift(
        &self,
    ) -> Result<Json<ObjectResponse<crate::drift::DriftReport>>, ErrorData> {
        let report = crate::drift::run(&self.project_root, &self.config)
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
        Ok(Json(ObjectResponse { result: report }))
    }

    // ------------------------------------------------------------------
    // Context layer (.hief/context/)
    // ------------------------------------------------------------------

    #[tool(
        name = "list_context_files",
        description = "List all context files in .hief/context/. Returns name, title, path, size, and last-modified timestamp. Use before read_context_file to discover available files."
    )]
    async fn list_context_files(
        &self,
    ) -> Result<Json<ObjectResponse<Vec<crate::context::ContextFile>>>, ErrorData> {
        let files = crate::context::list_context_files(&self.project_root);
        Ok(Json(ObjectResponse { result: files }))
    }

    #[tool(
        name = "read_context_file",
        description = "Read a context file from .hief/context/ by name (with or without .md). Returns full markdown content."
    )]
    async fn read_context_file(
        &self,
        Parameters(params): Parameters<ReadContextFileParams>,
    ) -> Result<Json<ObjectResponse<String>>, ErrorData> {
        let content = crate::context::read_context_file(&self.project_root, &params.name)
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
        Ok(Json(ObjectResponse { result: content }))
    }

    #[tool(
        name = "write_context_file",
        description = "Write or overwrite a context file in .hief/context/. Use for the GROW step: update architecture.md or decisions.md after completing a task. Creates directory if needed."
    )]
    async fn write_context_file(
        &self,
        Parameters(params): Parameters<WriteContextFileParams>,
    ) -> Result<Json<ObjectResponse<String>>, ErrorData> {
        crate::context::write_context_file(&self.project_root, &params.name, &params.content)
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
        Ok(Json(ObjectResponse {
            result: format!(
                "written: .hief/context/{}.md",
                params.name.trim_end_matches(".md")
            ),
        }))
    }

    // ------------------------------------------------------------------
    // Pattern library (.hief/patterns/)
    // ------------------------------------------------------------------

    #[tool(
        name = "list_patterns",
        description = "List all project patterns in .hief/patterns/. Patterns capture project-specific task guides with gotchas and verify checklists."
    )]
    async fn list_patterns(
        &self,
    ) -> Result<Json<ObjectResponse<Vec<crate::patterns::PatternSummary>>>, ErrorData> {
        let patterns = crate::patterns::list_patterns(&self.project_root);
        Ok(Json(ObjectResponse { result: patterns }))
    }

    #[tool(
        name = "get_pattern",
        description = "Read the full markdown content of a named project pattern (name without .md extension)."
    )]
    async fn get_pattern(
        &self,
        Parameters(params): Parameters<GetPatternParams>,
    ) -> Result<Json<ObjectResponse<String>>, ErrorData> {
        let content = crate::patterns::get_pattern(&self.project_root, &params.name)
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
        Ok(Json(ObjectResponse { result: content }))
    }

    #[tool(
        name = "create_pattern",
        description = "Create a new project pattern in .hief/patterns/. Auto-updates INDEX.md. Supply a kebab-case name, optional title, and optional markdown content (stub template created if omitted)."
    )]
    async fn create_pattern(
        &self,
        Parameters(params): Parameters<CreatePatternParams>,
    ) -> Result<Json<ObjectResponse<String>>, ErrorData> {
        let title = params.title.as_deref().unwrap_or(&params.name);
        let content = params.content.clone().unwrap_or_else(|| {
            format!(
                "# Pattern: {}\n\n## Steps\n\n1. <!-- TODO -->\n\n## Gotchas\n\n- <!-- TODO -->\n\n## Verify\n\n- [ ] <!-- TODO -->\n",
                title
            )
        });
        crate::patterns::create_pattern(&self.project_root, &params.name, &content)
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
        Ok(Json(ObjectResponse {
            result: format!(
                "created: .hief/patterns/{}.md",
                params.name.trim_end_matches(".md")
            ),
        }))
    }

    // ------------------------------------------------------------------
    // Routing table (.hief/router.toml)
    // ------------------------------------------------------------------

    #[tool(
        name = "get_routing_table",
        description = "Get the session routing table from .hief/router.toml. Maps task types to context files and patterns to load. Returns built-in defaults if no router.toml exists."
    )]
    async fn get_routing_table(
        &self,
    ) -> Result<Json<ObjectResponse<crate::router::RoutingTable>>, ErrorData> {
        let table = crate::router::load_routing_table(&self.project_root)
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
        Ok(Json(ObjectResponse { result: table }))
    }

    // ------------------------------------------------------------------
    // Skills management
    // ------------------------------------------------------------------

    #[tool(
        name = "reload_skills",
        description = "Hot-reload skill files from disk so newly added or updated skills are available without restarting the MCP server."
    )]
    async fn reload_skills(&self) -> Result<Json<ObjectResponse<ReloadSkillsResponse>>, ErrorData> {
        self.skills
            .load_from_disk(&self.project_root)
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;

        let defs = self.skills.tool_defs();
        let skill_names: Vec<String> = defs.iter().map(|t| t.name.to_string()).collect();
        Ok(Json(ObjectResponse {
            result: ReloadSkillsResponse {
                count: skill_names.len(),
                skill_names,
            },
        }))
    }

    // ------------------------------------------------------------------
    // Dynamic skill execution (wildcard)
    // ------------------------------------------------------------------

    #[tool(
        name = "execute_skill_*",
        description = "Return the markdown contents of a named skill. Replace '*' with the skill identifier. Requires `reason` parameter."
    )]
    async fn execute_dynamic_skill(
        &self,
        Parameters(_params): Parameters<HashMap<String, String>>,
        tool: ToolName,
    ) -> Result<Json<ObjectResponse<String>>, ErrorData> {
        let tool_name = tool.0.as_ref();
        if let Some(skill) = self.skills.by_tool(tool_name) {
            Ok(Json(ObjectResponse {
                result: skill.content,
            }))
        } else {
            Err(ErrorData::resource_not_found(
                format!("skill not found: {}", tool_name),
                None,
            ))
        }
    }
} // end #[tool_router] impl HiefServer

#[derive(Debug)]
struct CommandOutcome {
    stdout: String,
    stderr: String,
    exit_code: Option<i32>,
    timed_out: bool,
}

async fn run_shell_command(
    command: &str,
    cwd: &std::path::Path,
    timeout_secs: u64,
) -> crate::errors::Result<CommandOutcome> {
    run_command_with_input("sh", &["-c", command], None, cwd, timeout_secs, &[]).await
}

async fn run_command_with_input(
    bin: &str,
    args: &[&str],
    stdin_input: Option<&str>,
    cwd: &std::path::Path,
    timeout_secs: u64,
    envs: &[(&str, &str)],
) -> crate::errors::Result<CommandOutcome> {
    let mut cmd = tokio::process::Command::new(bin);
    cmd.args(args)
        .current_dir(cwd)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    if stdin_input.is_some() {
        cmd.stdin(Stdio::piped());
    }

    for (k, v) in envs {
        cmd.env(k, v);
    }

    let mut child = cmd
        .spawn()
        .map_err(|e| crate::errors::HiefError::Other(format!("spawn failed: {}", e)))?;

    if let Some(input) = stdin_input
        && let Some(mut stdin) = child.stdin.take()
    {
        use tokio::io::AsyncWriteExt;
        stdin
            .write_all(input.as_bytes())
            .await
            .map_err(|e| crate::errors::HiefError::Other(format!("stdin write failed: {}", e)))?;
    }

    let stdout_handle = child.stdout.take().map(|mut stdout| {
        tokio::spawn(async move {
            let mut buf = Vec::new();
            stdout.read_to_end(&mut buf).await.map(|_| buf)
        })
    });

    let stderr_handle = child.stderr.take().map(|mut stderr| {
        tokio::spawn(async move {
            let mut buf = Vec::new();
            stderr.read_to_end(&mut buf).await.map(|_| buf)
        })
    });

    let status_result = tokio::time::timeout(Duration::from_secs(timeout_secs), child.wait()).await;

    let (timed_out, exit_code) = match status_result {
        Ok(status_res) => {
            let status = status_res
                .map_err(|e| crate::errors::HiefError::Other(format!("wait failed: {}", e)))?;
            (false, status.code())
        }
        Err(_) => {
            let _ = child.kill().await;
            (true, None)
        }
    };

    let stdout = match stdout_handle {
        Some(handle) => match handle.await {
            Ok(Ok(bytes)) => String::from_utf8_lossy(&bytes).to_string(),
            Ok(Err(e)) => format!("<failed to read stdout: {}>", e),
            Err(e) => format!("<stdout task failed: {}>", e),
        },
        None => String::new(),
    };

    let stderr = match stderr_handle {
        Some(handle) => match handle.await {
            Ok(Ok(bytes)) => String::from_utf8_lossy(&bytes).to_string(),
            Ok(Err(e)) => format!("<failed to read stderr: {}>", e),
            Err(e) => format!("<stderr task failed: {}>", e),
        },
        None => String::new(),
    };

    Ok(CommandOutcome {
        stdout,
        stderr,
        exit_code,
        timed_out,
    })
}

fn collect_summary_lines(output: &str) -> Vec<String> {
    output
        .lines()
        .filter(|line| line.contains("test result:") || line.starts_with("running "))
        .map(ToOwned::to_owned)
        .collect()
}

fn tail_lines(text: &str, max_lines: usize) -> Vec<String> {
    let lines: Vec<String> = text.lines().map(ToOwned::to_owned).collect();
    if lines.len() <= max_lines {
        lines
    } else {
        lines[lines.len() - max_lines..].to_vec()
    }
}

fn build_judge_prompt(rubric: &str, input: &str) -> String {
    format!(
        "You are a strict software quality judge.\\n\\nRubric:\\n{}\\n\\nInput to judge:\\n{}\\n\\nReturn JSON only with keys: score_0_to_100 (number), verdict (string), rationale (string), risks (array of strings), suggestions (array of strings).",
        rubric, input
    )
}

fn parse_judge_output(raw: &str) -> Option<JudgeParsed> {
    let value = parse_json_from_maybe_wrapped_text(raw)?;

    let score = value
        .get("score_0_to_100")
        .and_then(|v| v.as_f64().or_else(|| v.as_i64().map(|i| i as f64)));
    let verdict = value
        .get("verdict")
        .and_then(serde_json::Value::as_str)
        .map(ToOwned::to_owned);
    let rationale = value
        .get("rationale")
        .and_then(serde_json::Value::as_str)
        .map(ToOwned::to_owned);
    let risks = value
        .get("risks")
        .and_then(serde_json::Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(serde_json::Value::as_str)
                .map(ToOwned::to_owned)
                .collect::<Vec<String>>()
        })
        .unwrap_or_default();
    let suggestions = value
        .get("suggestions")
        .and_then(serde_json::Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(serde_json::Value::as_str)
                .map(ToOwned::to_owned)
                .collect::<Vec<String>>()
        })
        .unwrap_or_default();

    Some(JudgeParsed {
        score_0_to_100: score,
        verdict,
        rationale,
        risks,
        suggestions,
    })
}

fn parse_json_from_maybe_wrapped_text(raw: &str) -> Option<serde_json::Value> {
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(raw.trim()) {
        return Some(v);
    }

    let start = raw.find('{')?;
    let end = raw.rfind('}')?;
    if start >= end {
        return None;
    }
    serde_json::from_str::<serde_json::Value>(&raw[start..=end]).ok()
}

/// Implement ServerHandler for the HIEF MCP server.
#[tool_handler]
impl ServerHandler for HiefServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: Default::default(),
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            server_info: Implementation::from_build_env(),
            instructions: Some(
                "HIEF is your local persistent memory layer for AI coding agents.\n\
                 • Search code with precision: keyword, structural (AST), or semantic vectors.\n\
                 • Coordinate work using a lightweight intent graph (create, list, update, recover).\n\
                 • Enforce quality with golden-set evaluation before sharing or merging changes.\n\
                 • Always begin a session by calling `get_project_context` then `get_conventions` \
                   to bootstrap your agent's view of the repo and coding rules.\n\
                 • Most tools return `ObjectResponse` wrappers; check `result` field.\n\
                 • For detailed examples and full protocol, see dev-docs/agent-protocol.md."
                    .to_string(),
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use tempfile::tempdir;

    #[test]
    fn judge_parser_handles_wrapped_json() {
        let raw = "model output... {\"score_0_to_100\": 88, \"verdict\": \"pass\", \"rationale\": \"solid\", \"risks\": [\"edge cases\"], \"suggestions\": [\"add tests\"]}";
        let parsed = parse_judge_output(raw).expect("parse failed");
        assert_eq!(parsed.verdict.as_deref(), Some("pass"));
        assert_eq!(parsed.risks.len(), 1);
    }

    #[test]
    fn summary_lines_extract_test_markers() {
        let lines =
            collect_summary_lines("running 2 tests\nfoo\ntest result: ok. 2 passed; 0 failed;\n");
        assert_eq!(lines.len(), 2);
    }

    #[tokio::test]
    async fn create_intent_with_skill_returns_skill_content() {
        let tmp = tempdir().expect("failed to create tempdir");
        let db_path = tmp.path().join("hief.db");
        let db = Database::open(&db_path).await.expect("db operation failed");
        let server = HiefServer::new(db.clone(), tmp.path().to_path_buf());

        // create skills directory and file
        let config = Config::default();
        let skills_dir = tmp.path().join(&config.skills.skills_path);
        std::fs::create_dir_all(&skills_dir).expect("failed to create skills_dir");
        std::fs::write(skills_dir.join("foo.md"), "do something")
            .expect("failed to write skill file");

        let params = CreateIntentParams {
            kind: "feature".to_string(),
            title: "Add foo".to_string(),
            description: None,
            priority: None,
            depends_on: None,
            skill: Some("foo".to_string()),
        };
        let result = server.create_intent(Parameters(params)).await;
        assert!(result.is_ok());
        let Json(val) = result.expect("create_intent result error");
        let json_str = serde_json::to_string(&val).expect("JSON serialization failed");
        assert!(json_str.contains("skill_content"));
        assert!(json_str.contains("do something"));
    }

    #[tokio::test]
    async fn dynamic_skill_registry_and_handler() {
        let tmp = tempdir().expect("failed to create tempdir");
        let db_path = tmp.path().join("hief.db");
        let db = Database::open(&db_path).await.expect("db operation failed");
        // create a skill file before server startup
        let config = Config::default();
        let skills_dir = tmp.path().join(&config.skills.skills_path);
        std::fs::create_dir_all(&skills_dir).expect("failed to create skills_dir");
        std::fs::write(skills_dir.join("foo.md"), "# Foo\nstep1")
            .expect("failed to write skill file");

        let server = HiefServer::new(db.clone(), tmp.path().to_path_buf());

        // registry should contain execute_skill_foo
        let opt = server.skills.by_tool("execute_skill_foo");
        assert!(opt.is_some());
        assert!(opt.expect("skill tool missing").content.contains("step1"));

        // calling the generic handler directly
        let params: HashMap<String, String> = [("reason".to_string(), "test".to_string())]
            .into_iter()
            .collect();
        let resp = server
            .execute_dynamic_skill(Parameters(params), ToolName("execute_skill_foo".into()))
            .await
            .expect("execute_dynamic_skill failed");
        let Json(ObjectResponse { result: text }) = resp;
        assert!(text.contains("step1"));
    }

    #[tokio::test]
    async fn reload_skills_picks_up_new_content() {
        let tmp = tempdir().expect("failed to create tempdir");
        let db_path = tmp.path().join("hief.db");
        let db = Database::open(&db_path).await.expect("db operation failed");
        // create skill file BEFORE server start so initial load succeeds
        let config = Config::default();
        let skills_dir = tmp.path().join(&config.skills.skills_path);
        std::fs::create_dir_all(&skills_dir).expect("failed to create skills_dir");
        std::fs::write(skills_dir.join("deploy.md"), "# Deploy\nold step")
            .expect("failed to write skill file");

        let server = HiefServer::new(db.clone(), tmp.path().to_path_buf());
        assert!(server.skills.by_tool("execute_skill_deploy").is_some());

        // overwrite with updated content
        std::fs::write(skills_dir.join("deploy.md"), "# Deploy\nnew step")
            .expect("failed to write skill file");

        // reload_skills should update the registry
        let resp = server.reload_skills().await.expect("reload_skills failed");
        let Json(ObjectResponse { result: body }) = resp;
        assert_eq!(body.count, 1);
        assert!(
            body.skill_names
                .contains(&"execute_skill_deploy".to_string())
        );

        // registry entry should reflect new content
        let skill = server
            .skills
            .by_tool("execute_skill_deploy")
            .expect("skill missing");
        assert!(skill.content.contains("new step"));
    }
}
