//! MCP tool definitions for the HIEF server.

use std::path::PathBuf;

use rmcp::{ServerHandler, tool, tool_router, tool_handler, ErrorData};
use rmcp::handler::server::tool::ToolRouter;
use rmcp::handler::server::wrapper::{Json, Parameters};
use rmcp::model::{ServerCapabilities, ServerInfo, Implementation};
use rmcp::schemars;
use serde::Deserialize;

use crate::config::Config;
use crate::db::Database;
use crate::graph;
use crate::graph::edges::IntentEdge;
use crate::graph::intent::Intent;
use crate::index;
use crate::index::search::SearchQuery;

/// The HIEF MCP server handler.
#[derive(Clone)]
pub struct HiefServer {
    db: Database,
    project_root: PathBuf,
    tool_router: ToolRouter<Self>,
}

impl HiefServer {
    pub fn new(db: Database, project_root: PathBuf) -> Self {
        let tool_router = Self::tool_router();
        Self { db, project_root, tool_router }
    }
}

// -- Parameter structs --

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

#[tool_router]
impl HiefServer {
    #[tool(
        name = "search_code",
        description = "Search indexed code. Returns matching chunks with file paths, symbol names, and snippets. Supports FTS5 syntax: prefix*, phrases, AND/OR, column:filter."
    )]
    async fn search_code(
        &self,
        Parameters(params): Parameters<SearchCodeParams>,
    ) -> Result<Json<String>, ErrorData> {
        let mut search_query = SearchQuery::new(&params.query);
        search_query.top_k = params.top_k.unwrap_or(10);
        search_query.language = params.language;
        search_query.symbol_kind = params.symbol_kind;

        let results = index::search(&self.db, &search_query)
            .await
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;

        let json = serde_json::to_string_pretty(&results)
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
        Ok(Json(json))
    }

    #[tool(
        name = "index_status",
        description = "Get the current index statistics: file count, chunk count, languages, last indexed time, and database size."
    )]
    async fn index_status(&self) -> Result<Json<String>, ErrorData> {
        let stats = index::status(&self.db, &self.project_root)
            .await
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;

        let json = serde_json::to_string_pretty(&stats)
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
        Ok(Json(json))
    }

    #[tool(
        name = "create_intent",
        description = "Create a new intent (task) in the dependency graph. Returns the created intent with its UUID."
    )]
    async fn create_intent(
        &self,
        Parameters(params): Parameters<CreateIntentParams>,
    ) -> Result<Json<String>, ErrorData> {
        let intent = Intent::new(params.kind, params.title, params.description, params.priority);

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

        let json = serde_json::to_string_pretty(&intent)
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
        Ok(Json(json))
    }

    #[tool(
        name = "list_intents",
        description = "List intents in the dependency graph, optionally filtered by status and/or kind."
    )]
    async fn list_intents(
        &self,
        Parameters(params): Parameters<ListIntentsParams>,
    ) -> Result<Json<String>, ErrorData> {
        let intents = graph::list_intents(&self.db, params.status.as_deref(), params.kind.as_deref())
            .await
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;

        let json = serde_json::to_string_pretty(&intents)
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
        Ok(Json(json))
    }

    #[tool(
        name = "update_intent",
        description = "Update an intent status or assignment. Status transitions are validated."
    )]
    async fn update_intent(
        &self,
        Parameters(params): Parameters<UpdateIntentParams>,
    ) -> Result<Json<String>, ErrorData> {
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

        let json = serde_json::to_string_pretty(&intent)
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
        Ok(Json(json))
    }

    #[tool(
        name = "ready_intents",
        description = "Show intents that are approved and whose all dependencies are satisfied."
    )]
    async fn ready_intents(&self) -> Result<Json<String>, ErrorData> {
        let intents = graph::ready_intents(&self.db)
            .await
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;

        let json = serde_json::to_string_pretty(&intents)
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
        Ok(Json(json))
    }

    #[tool(
        name = "run_evaluation",
        description = "Run golden set evaluation against the indexed codebase."
    )]
    async fn run_evaluation(
        &self,
        Parameters(params): Parameters<RunEvaluationParams>,
    ) -> Result<Json<String>, ErrorData> {
        let config = Config::load(&self.project_root.join("hief.toml"))
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;

        let results = crate::eval::run(&self.db, &self.project_root, &config.eval, params.golden_set.as_deref())
            .await
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;

        let json = serde_json::to_string_pretty(&results)
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
        Ok(Json(json))
    }

    #[tool(
        name = "get_eval_scores",
        description = "Get evaluation score history for a golden set."
    )]
    async fn get_eval_scores(
        &self,
        Parameters(params): Parameters<GetEvalScoresParams>,
    ) -> Result<Json<String>, ErrorData> {
        let history = crate::eval::history::get_history(&self.db, &params.golden_set, params.limit.unwrap_or(10))
            .await
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;

        let json = serde_json::to_string_pretty(&history)
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
        Ok(Json(json))
    }

    #[tool(
        name = "get_project_context",
        description = "Get a high-level project overview: index stats, active intents, and ready intents."
    )]
    async fn get_project_context(&self) -> Result<Json<String>, ErrorData> {
        let stats = index::status(&self.db, &self.project_root)
            .await
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;

        let active_intents = graph::list_intents(&self.db, Some("in_progress"), None)
            .await
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;

        let ready = graph::ready_intents(&self.db)
            .await
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;

        let context = serde_json::json!({
            "index": stats,
            "active_intents": active_intents,
            "ready_intents": ready,
        });

        let json = serde_json::to_string_pretty(&context)
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
        Ok(Json(json))
    }

    #[tool(
        name = "git_blame",
        description = "Get git blame information for a file range."
    )]
    async fn git_blame(
        &self,
        Parameters(params): Parameters<GitBlameParams>,
    ) -> Result<Json<String>, ErrorData> {
        let result = crate::index::search::git_blame_range(&params.file, params.start_line, params.end_line)
            .await
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
        Ok(Json(result))
    }
}

/// Implement ServerHandler for the HIEF MCP server.
#[tool_handler]
impl ServerHandler for HiefServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: Default::default(),
            capabilities: ServerCapabilities::builder()
                .enable_tools()
                .build(),
            server_info: Implementation::from_build_env(),
            instructions: Some(
                "HIEF (Hybrid Intent-Evaluation Framework) provides code indexing, \
                 intent tracking, and evaluation scoring for AI coding agents."
                    .to_string(),
            ),
        }
    }
}
