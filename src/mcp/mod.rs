//! MCP server setup and tool definitions.

pub mod resources;
pub mod skills;
pub mod tools;

#[allow(unused_imports)]
pub use tools::{CorrectionHint, ToolValidationPayload};

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

/// Build the HTTP router: `/mcp` (Streamable HTTP) and `/health`.
fn http_router(server: tools::HiefServer, host: &str, port: u16) -> axum::Router {
    // rmcp's default allowlist is loopback-only, which blocks DNS rebinding
    // (GHSA-89vp-x53w-74fx). A deliberate non-loopback bind must also be allowed.
    let mut config = rmcp::transport::StreamableHttpServerConfig::default();
    if !matches!(host, "localhost" | "127.0.0.1" | "::1") {
        config.allowed_hosts.push(host.to_string());
        config.allowed_hosts.push(format!("{host}:{port}"));
    }
    let session_manager = std::sync::Arc::new(
        rmcp::transport::streamable_http_server::session::local::LocalSessionManager::default(),
    );
    let service = rmcp::transport::StreamableHttpService::new(
        move || Ok(server.clone()),
        session_manager,
        config,
    );

    axum::Router::new()
        .nest_service("/mcp", service)
        .route("/health", axum::routing::get(|| async { "ok" }))
}

async fn start_http(server: tools::HiefServer, host: &str, port: u16) -> Result<()> {
    let app = http_router(server, host, port);

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

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::util::ServiceExt;

    async fn post_mcp(host_header: &str) -> StatusCode {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = crate::db::Database::open(&dir.path().join("hief.db"))
            .await
            .expect("open db");
        let server = tools::HiefServer::new(db, dir.path().to_path_buf());
        let req = Request::post("/mcp")
            .header("host", host_header)
            .header("content-type", "application/json")
            .header("accept", "application/json, text/event-stream")
            .body(Body::from("{}"))
            .expect("request");
        http_router(server, "127.0.0.1", 3100)
            .oneshot(req)
            .await
            .expect("response")
            .status()
    }

    /// A DNS-rebinding request carries a non-local `Host` and must be refused.
    #[tokio::test]
    async fn rejects_non_local_host_header() {
        assert_eq!(post_mcp("evil.example.com").await, StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn accepts_local_host_header() {
        assert_ne!(post_mcp("localhost:3100").await, StatusCode::FORBIDDEN);
    }
}
