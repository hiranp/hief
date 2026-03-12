//! MCP tool definitions for the HIEF server.

use std::collections::HashMap;
use std::path::PathBuf;

use rmcp::handler::server::tool::ToolRouter;
use rmcp::handler::server::wrapper::{Json, Parameters};
use rmcp::model::{Implementation, ServerCapabilities, ServerInfo};
use rmcp::schemars;
use rmcp::{ErrorData, ServerHandler, tool, tool_handler, tool_router};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

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
    pub message: String,
    pub query: String,
    pub top_k: usize,
    pub language: Option<String>,
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

        // TODO: Generate query embedding via host agent callback or local model
        // For now, return a helpful message indicating the feature is being built
        let resp = SemanticSearchResponse {
            status: "not_yet_available".to_string(),
            message: "Semantic search is enabled in config but the LanceDB integration is still being built. Use search_code (keyword) or structural_search (AST pattern) in the meantime.".to_string(),
            query: params.query.clone(),
            top_k: self.validate_top_k(params.top_k, 10),
            language: params.language.clone(),
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
    // Skills management
    // ------------------------------------------------------------------

    #[tool(
        name = "reload_skills",
        description = "Hot-reload skill files from .hief/skills/ without restarting the server. \
                       Content changes to existing skills are reflected immediately. \
                       Returns the updated list of loaded skill tool-names."
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
