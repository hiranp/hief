//! MCP server setup and tool definitions.

pub mod resources;
pub mod tools;

use std::path::PathBuf;
use tracing::info;

use crate::config::ServeConfig;
use crate::db::Database;
use crate::errors::Result;

/// Start the MCP server with the configured transport.
pub async fn start(
    db: Database,
    project_root: PathBuf,
    config: &ServeConfig,
    transport_override: Option<&str>,
    port_override: Option<u16>,
) -> Result<()> {
    let transport = transport_override.unwrap_or(&config.transport);
    let port = port_override.unwrap_or(config.port);

    let server = tools::HiefServer::new(db, project_root);

    match transport {
        "stdio" => {
            info!("Starting MCP server on stdio");
            start_stdio(server).await?;
        }
        "http" => {
            info!("Starting MCP server on http://{}:{}", config.host, port);
            start_http(server, &config.host, port).await?;
        }
        other => {
            return Err(crate::errors::HiefError::Config(format!(
                "Unknown transport: {}",
                other
            )));
        }
    }

    Ok(())
}

async fn start_stdio(server: tools::HiefServer) -> Result<()> {
    let transport = rmcp::transport::io::stdio();
    let _running = rmcp::serve_server(server, transport)
        .await
        .map_err(|e| crate::errors::HiefError::Other(format!("MCP server error: {}", e)))?;
    // Keep running until ctrl-c
    tokio::signal::ctrl_c()
        .await
        .map_err(|e| crate::errors::HiefError::Other(format!("signal error: {}", e)))?;
    Ok(())
}

async fn start_http(
    server: tools::HiefServer,
    host: &str,
    port: u16,
) -> Result<()> {
    let config = rmcp::transport::StreamableHttpServerConfig::default();
    let session_manager = std::sync::Arc::new(
        rmcp::transport::streamable_http_server::session::local::LocalSessionManager::default()
    );
    let service = rmcp::transport::StreamableHttpService::new(
        move || Ok(server.clone()),
        session_manager,
        config,
    );

    let app = axum::Router::new()
        .nest_service("/mcp", service)
        .route("/health", axum::routing::get(|| async { "ok" }));

    let addr = format!("{}:{}", host, port);
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .map_err(|e| crate::errors::HiefError::Other(format!("Failed to bind {}: {}", addr, e)))?;

    info!("MCP HTTP server listening on {}", addr);
    axum::serve(listener, app)
        .await
        .map_err(|e| crate::errors::HiefError::Other(format!("HTTP server error: {}", e)))?;

    Ok(())
}
