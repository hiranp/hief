//! MCP server registration for popular AI coding frameworks.
//!
//! Automatically configures HIEF as an MCP server in Claude CLI, Claude Desktop,
//! VS Code, Cursor, Windsurf, Gemini CLI, and other MCP-compatible tools.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use crate::errors::Result;

/// Supported MCP client frameworks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McpClient {
    ClaudeCli,
    ClaudeDesktop,
    VsCode,
    Cursor,
    Windsurf,
    GeminiCli,
}

impl McpClient {
    /// All known client frameworks.
    pub fn all() -> &'static [McpClient] {
        &[
            McpClient::ClaudeCli,
            McpClient::ClaudeDesktop,
            McpClient::VsCode,
            McpClient::Cursor,
            McpClient::Windsurf,
            McpClient::GeminiCli,
        ]
    }

    /// Human-readable name.
    pub fn name(&self) -> &'static str {
        match self {
            McpClient::ClaudeCli => "Claude CLI (claude code)",
            McpClient::ClaudeDesktop => "Claude Desktop",
            McpClient::VsCode => "VS Code (GitHub Copilot)",
            McpClient::Cursor => "Cursor",
            McpClient::Windsurf => "Windsurf",
            McpClient::GeminiCli => "Gemini CLI",
        }
    }

    /// Short CLI identifier.
    pub fn id(&self) -> &'static str {
        match self {
            McpClient::ClaudeCli => "claude-cli",
            McpClient::ClaudeDesktop => "claude-desktop",
            McpClient::VsCode => "vscode",
            McpClient::Cursor => "cursor",
            McpClient::Windsurf => "windsurf",
            McpClient::GeminiCli => "gemini-cli",
        }
    }

    /// Parse from CLI string.
    pub fn from_str(s: &str) -> Option<McpClient> {
        match s {
            "claude-cli" | "claude" | "claude-code" => Some(McpClient::ClaudeCli),
            "claude-desktop" => Some(McpClient::ClaudeDesktop),
            "vscode" | "vs-code" | "code" => Some(McpClient::VsCode),
            "cursor" => Some(McpClient::Cursor),
            "windsurf" => Some(McpClient::Windsurf),
            "gemini-cli" | "gemini" => Some(McpClient::GeminiCli),
            "all" => None, // handled separately
            _ => None,
        }
    }

    /// Config file path for this client (global or project-level).
    fn config_path(&self, project_root: &Path, scope: ConfigScope) -> Option<PathBuf> {
        let home = dirs::home_dir()?;

        match (self, scope) {
            // Claude CLI: project-level .mcp.json
            (McpClient::ClaudeCli, ConfigScope::Project) => Some(project_root.join(".mcp.json")),
            // Claude CLI: global ~/.claude.json
            (McpClient::ClaudeCli, ConfigScope::Global) => Some(home.join(".claude.json")),

            // Claude Desktop: global only
            (McpClient::ClaudeDesktop, ConfigScope::Global) => {
                #[cfg(target_os = "macos")]
                {
                    Some(home.join("Library/Application Support/Claude/claude_desktop_config.json"))
                }
                #[cfg(not(target_os = "macos"))]
                {
                    Some(home.join(".config/Claude/claude_desktop_config.json"))
                }
            }
            (McpClient::ClaudeDesktop, ConfigScope::Project) => None,

            // VS Code: project-level .vscode/mcp.json
            (McpClient::VsCode, ConfigScope::Project) => {
                Some(project_root.join(".vscode").join("mcp.json"))
            }
            (McpClient::VsCode, ConfigScope::Global) => None,

            // Cursor: project-level .cursor/mcp.json or global
            (McpClient::Cursor, ConfigScope::Project) => {
                Some(project_root.join(".cursor").join("mcp.json"))
            }
            (McpClient::Cursor, ConfigScope::Global) => Some(home.join(".cursor").join("mcp.json")),

            // Windsurf: global ~/.codeium/windsurf/mcp_config.json
            (McpClient::Windsurf, ConfigScope::Global) => Some(
                home.join(".codeium")
                    .join("windsurf")
                    .join("mcp_config.json"),
            ),
            (McpClient::Windsurf, ConfigScope::Project) => None,

            // Gemini CLI: global ~/.gemini/settings.json
            (McpClient::GeminiCli, ConfigScope::Global) => {
                Some(home.join(".gemini").join("settings.json"))
            }
            (McpClient::GeminiCli, ConfigScope::Project) => None,
        }
    }
}

/// Whether to install at project level or globally.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigScope {
    Project,
    Global,
}

/// Result of an install/uninstall/status operation for one client.
#[derive(Debug)]
pub struct McpRegistration {
    pub client: &'static str,
    pub config_path: PathBuf,
    pub status: RegistrationStatus,
}

#[derive(Debug)]
pub enum RegistrationStatus {
    Installed,
    AlreadyInstalled,
    Uninstalled,
    NotInstalled,
    Created,
    Error(String),
}

impl std::fmt::Display for RegistrationStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RegistrationStatus::Installed => write!(f, "✅ installed"),
            RegistrationStatus::AlreadyInstalled => write!(f, "✅ already installed"),
            RegistrationStatus::Uninstalled => write!(f, "🗑  uninstalled"),
            RegistrationStatus::NotInstalled => write!(f, "—  not installed"),
            RegistrationStatus::Created => write!(f, "✅ created"),
            RegistrationStatus::Error(e) => write!(f, "❌ error: {}", e),
        }
    }
}

/// Build the HIEF MCP server entry for JSON configs.
fn hief_server_entry(project_root: &Path) -> Value {
    let hief_binary = std::env::current_exe()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| "hief".to_string());

    json!({
        "command": hief_binary,
        "args": ["serve"],
        "cwd": project_root.to_string_lossy(),
        "env": {}
    })
}

/// Build the HIEF entry for VS Code mcp.json format (uses `type` field).
fn hief_vscode_entry(project_root: &Path) -> Value {
    let hief_binary = std::env::current_exe()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| "hief".to_string());

    json!({
        "type": "stdio",
        "command": hief_binary,
        "args": ["serve"],
        "cwd": project_root.to_string_lossy()
    })
}

/// Read a JSON config file, or return an empty object if it doesn't exist.
fn read_json_config(path: &Path) -> Value {
    if path.exists() {
        match std::fs::read_to_string(path) {
            Ok(content) => {
                // Handle empty files
                if content.trim().is_empty() {
                    return json!({});
                }
                serde_json::from_str(&content).unwrap_or_else(|_| json!({}))
            }
            Err(_) => json!({}),
        }
    } else {
        json!({})
    }
}

/// Write a JSON config file, creating parent directories.
fn write_json_config(path: &Path, value: &Value) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let content = serde_json::to_string_pretty(value)
        .map_err(|e| crate::errors::HiefError::Other(e.to_string()))?;
    std::fs::write(path, content + "\n")?;
    Ok(())
}

/// Check if HIEF is already registered in a JSON config.
fn is_hief_registered(config: &Value) -> bool {
    // Check mcpServers.hief or servers.hief
    if let Some(servers) = config.get("mcpServers").and_then(|v| v.as_object()) {
        return servers.contains_key("hief");
    }
    if let Some(servers) = config.get("servers").and_then(|v| v.as_object()) {
        return servers.contains_key("hief");
    }
    false
}

/// Install HIEF into a specific client's config.
fn install_for_client(
    client: McpClient,
    project_root: &Path,
    scope: ConfigScope,
) -> McpRegistration {
    let config_path = match client.config_path(project_root, scope) {
        Some(p) => p,
        None => {
            return McpRegistration {
                client: client.name(),
                config_path: PathBuf::from("N/A"),
                status: RegistrationStatus::Error(format!(
                    "{} does not support {:?} scope",
                    client.name(),
                    scope
                )),
            };
        }
    };

    let mut config = read_json_config(&config_path);

    // Check if already installed
    if is_hief_registered(&config) {
        return McpRegistration {
            client: client.name(),
            config_path,
            status: RegistrationStatus::AlreadyInstalled,
        };
    }

    // Determine the server key and entry format
    let (server_key, entry) = match client {
        McpClient::VsCode => ("servers", hief_vscode_entry(project_root)),
        _ => ("mcpServers", hief_server_entry(project_root)),
    };

    // Ensure the servers object exists
    if config.get(server_key).is_none() {
        config[server_key] = json!({});
    }

    // Add HIEF entry
    config[server_key]["hief"] = entry;

    // Write back
    match write_json_config(&config_path, &config) {
        Ok(_) => {
            let status = if !config_path.exists() {
                RegistrationStatus::Created
            } else {
                RegistrationStatus::Installed
            };
            McpRegistration {
                client: client.name(),
                config_path,
                status,
            }
        }
        Err(e) => McpRegistration {
            client: client.name(),
            config_path,
            status: RegistrationStatus::Error(e.to_string()),
        },
    }
}

/// Uninstall HIEF from a specific client's config.
fn uninstall_for_client(
    client: McpClient,
    project_root: &Path,
    scope: ConfigScope,
) -> McpRegistration {
    let config_path = match client.config_path(project_root, scope) {
        Some(p) => p,
        None => {
            return McpRegistration {
                client: client.name(),
                config_path: PathBuf::from("N/A"),
                status: RegistrationStatus::NotInstalled,
            };
        }
    };

    if !config_path.exists() {
        return McpRegistration {
            client: client.name(),
            config_path,
            status: RegistrationStatus::NotInstalled,
        };
    }

    let mut config = read_json_config(&config_path);

    if !is_hief_registered(&config) {
        return McpRegistration {
            client: client.name(),
            config_path,
            status: RegistrationStatus::NotInstalled,
        };
    }

    // Remove HIEF from mcpServers or servers
    for key in &["mcpServers", "servers"] {
        if let Some(servers) = config.get_mut(key).and_then(|v| v.as_object_mut()) {
            servers.remove("hief");
        }
    }

    match write_json_config(&config_path, &config) {
        Ok(_) => McpRegistration {
            client: client.name(),
            config_path,
            status: RegistrationStatus::Uninstalled,
        },
        Err(e) => McpRegistration {
            client: client.name(),
            config_path,
            status: RegistrationStatus::Error(e.to_string()),
        },
    }
}

/// Check HIEF registration status for a specific client.
fn status_for_client(
    client: McpClient,
    project_root: &Path,
    scope: ConfigScope,
) -> McpRegistration {
    let config_path = match client.config_path(project_root, scope) {
        Some(p) => p,
        None => {
            return McpRegistration {
                client: client.name(),
                config_path: PathBuf::from("N/A"),
                status: RegistrationStatus::NotInstalled,
            };
        }
    };

    if !config_path.exists() {
        return McpRegistration {
            client: client.name(),
            config_path,
            status: RegistrationStatus::NotInstalled,
        };
    }

    let config = read_json_config(&config_path);

    let status = if is_hief_registered(&config) {
        RegistrationStatus::AlreadyInstalled
    } else {
        RegistrationStatus::NotInstalled
    };

    McpRegistration {
        client: client.name(),
        config_path,
        status,
    }
}

// ---------------------------------------------------------------------------
// Public CLI entry points
// ---------------------------------------------------------------------------

/// Install HIEF MCP server in one or all frameworks.
pub fn mcp_install(
    project_root: &Path,
    target: Option<&str>,
    global: bool,
    json_output: bool,
) -> Result<()> {
    let scope = if global {
        ConfigScope::Global
    } else {
        ConfigScope::Project
    };

    let clients: Vec<McpClient> = match target {
        Some("all") | None => {
            // For "all" or default, install in appropriate scope for each
            McpClient::all().to_vec()
        }
        Some(name) => match McpClient::from_str(name) {
            Some(client) => vec![client],
            None => {
                eprintln!("Unknown client '{}'. Available clients:", name);
                for c in McpClient::all() {
                    eprintln!("  {} ({})", c.id(), c.name());
                }
                return Ok(());
            }
        },
    };

    let mut results = Vec::new();

    for client in &clients {
        // For "all" mode, pick the natural scope for each client
        let actual_scope = if target == Some("all") || target.is_none() {
            match client {
                McpClient::ClaudeCli | McpClient::VsCode | McpClient::Cursor => {
                    ConfigScope::Project
                }
                _ => ConfigScope::Global,
            }
        } else {
            scope
        };

        results.push(install_for_client(*client, project_root, actual_scope));
    }

    print_results(&results, "install", json_output);
    Ok(())
}

/// Uninstall HIEF MCP server from one or all frameworks.
pub fn mcp_uninstall(
    project_root: &Path,
    target: Option<&str>,
    global: bool,
    json_output: bool,
) -> Result<()> {
    let scope = if global {
        ConfigScope::Global
    } else {
        ConfigScope::Project
    };

    let clients: Vec<McpClient> = match target {
        Some("all") | None => McpClient::all().to_vec(),
        Some(name) => match McpClient::from_str(name) {
            Some(client) => vec![client],
            None => {
                eprintln!("Unknown client '{}'", name);
                return Ok(());
            }
        },
    };

    let mut results = Vec::new();

    for client in &clients {
        let actual_scope = if target == Some("all") || target.is_none() {
            match client {
                McpClient::ClaudeCli | McpClient::VsCode | McpClient::Cursor => {
                    ConfigScope::Project
                }
                _ => ConfigScope::Global,
            }
        } else {
            scope
        };

        results.push(uninstall_for_client(*client, project_root, actual_scope));
    }

    print_results(&results, "uninstall", json_output);
    Ok(())
}

/// Show MCP registration status for all frameworks.
pub fn mcp_status(project_root: &Path, json_output: bool) -> Result<()> {
    let mut results = Vec::new();

    for client in McpClient::all() {
        // Check both project and global scope
        let project_scope = client.config_path(project_root, ConfigScope::Project);
        let global_scope = client.config_path(project_root, ConfigScope::Global);

        if project_scope.is_some() {
            results.push(status_for_client(
                *client,
                project_root,
                ConfigScope::Project,
            ));
        }
        if global_scope.is_some() {
            // Only add global if it's different from project
            let reg = status_for_client(*client, project_root, ConfigScope::Global);
            if !results
                .iter()
                .any(|r: &McpRegistration| r.config_path == reg.config_path)
            {
                results.push(reg);
            }
        }
    }

    print_results(&results, "status", json_output);
    Ok(())
}

/// Print results as human-readable or JSON.
fn print_results(results: &[McpRegistration], action: &str, json_output: bool) {
    if json_output {
        let entries: Vec<Value> = results
            .iter()
            .map(|r| {
                json!({
                    "client": r.client,
                    "config_path": r.config_path.to_string_lossy(),
                    "status": format!("{:?}", r.status),
                })
            })
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({ "action": action, "results": entries }))
                .unwrap_or_default()
        );
    } else {
        println!("\n  HIEF MCP Server — {}\n", action);
        for r in results {
            println!(
                "  {:<30} {} ({})",
                r.client,
                r.status,
                r.config_path.display()
            );
        }
        println!();

        if action == "install" {
            let installed = results.iter().any(|r| {
                matches!(
                    r.status,
                    RegistrationStatus::Installed | RegistrationStatus::Created
                )
            });
            if installed {
                println!("  Next: Restart your IDE/CLI to pick up the new MCP server.");
                println!("  HIEF will start automatically when your agent connects.\n");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_from_str() {
        assert_eq!(
            McpClient::from_str("claude-cli"),
            Some(McpClient::ClaudeCli)
        );
        assert_eq!(McpClient::from_str("claude"), Some(McpClient::ClaudeCli));
        assert_eq!(
            McpClient::from_str("claude-code"),
            Some(McpClient::ClaudeCli)
        );
        assert_eq!(McpClient::from_str("vscode"), Some(McpClient::VsCode));
        assert_eq!(McpClient::from_str("cursor"), Some(McpClient::Cursor));
        assert_eq!(McpClient::from_str("windsurf"), Some(McpClient::Windsurf));
        assert_eq!(
            McpClient::from_str("gemini-cli"),
            Some(McpClient::GeminiCli)
        );
        assert_eq!(McpClient::from_str("gemini"), Some(McpClient::GeminiCli));
        assert_eq!(McpClient::from_str("unknown"), None);
    }

    #[test]
    fn test_hief_server_entry_has_required_fields() {
        let entry = hief_server_entry(Path::new("/project"));
        assert!(entry.get("command").is_some());
        assert_eq!(entry["args"][0], "serve");
    }

    #[test]
    fn test_hief_vscode_entry_has_type() {
        let entry = hief_vscode_entry(Path::new("/project"));
        assert_eq!(entry["type"], "stdio");
        assert!(entry.get("command").is_some());
    }

    #[test]
    fn test_is_hief_registered() {
        let empty = json!({});
        assert!(!is_hief_registered(&empty));

        let with_hief = json!({"mcpServers": {"hief": {}}});
        assert!(is_hief_registered(&with_hief));

        let with_vscode_hief = json!({"servers": {"hief": {}}});
        assert!(is_hief_registered(&with_vscode_hief));

        let without_hief = json!({"mcpServers": {"other": {}}});
        assert!(!is_hief_registered(&without_hief));
    }

    #[test]
    fn test_install_creates_config() {
        let dir = tempfile::tempdir().unwrap();
        let project_root = dir.path();

        // Install for VS Code (project scope)
        let result = install_for_client(McpClient::VsCode, project_root, ConfigScope::Project);
        assert!(
            matches!(
                result.status,
                RegistrationStatus::Installed | RegistrationStatus::Created
            ),
            "Expected installed, got: {:?}",
            result.status
        );

        // Verify config file was created
        let config_path = project_root.join(".vscode").join("mcp.json");
        assert!(config_path.exists());

        let config: Value =
            serde_json::from_str(&std::fs::read_to_string(&config_path).unwrap()).unwrap();
        assert!(config["servers"]["hief"].is_object());
        assert_eq!(config["servers"]["hief"]["type"], "stdio");
    }

    #[test]
    fn test_install_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let project_root = dir.path();

        // Install twice
        install_for_client(McpClient::VsCode, project_root, ConfigScope::Project);
        let result = install_for_client(McpClient::VsCode, project_root, ConfigScope::Project);
        assert!(matches!(
            result.status,
            RegistrationStatus::AlreadyInstalled
        ));
    }

    #[test]
    fn test_uninstall_removes_entry() {
        let dir = tempfile::tempdir().unwrap();
        let project_root = dir.path();

        // Install then uninstall
        install_for_client(McpClient::VsCode, project_root, ConfigScope::Project);
        let result = uninstall_for_client(McpClient::VsCode, project_root, ConfigScope::Project);
        assert!(matches!(result.status, RegistrationStatus::Uninstalled));

        // Verify config no longer has hief
        let config_path = project_root.join(".vscode").join("mcp.json");
        let config: Value =
            serde_json::from_str(&std::fs::read_to_string(&config_path).unwrap()).unwrap();
        assert!(!is_hief_registered(&config));
    }

    #[test]
    fn test_status_reports_correctly() {
        let dir = tempfile::tempdir().unwrap();
        let project_root = dir.path();

        // Before install
        let result = status_for_client(McpClient::VsCode, project_root, ConfigScope::Project);
        assert!(matches!(result.status, RegistrationStatus::NotInstalled));

        // After install
        install_for_client(McpClient::VsCode, project_root, ConfigScope::Project);
        let result = status_for_client(McpClient::VsCode, project_root, ConfigScope::Project);
        assert!(matches!(
            result.status,
            RegistrationStatus::AlreadyInstalled
        ));
    }

    #[test]
    fn test_claude_cli_project_config() {
        let dir = tempfile::tempdir().unwrap();
        let project_root = dir.path();

        let result = install_for_client(McpClient::ClaudeCli, project_root, ConfigScope::Project);
        assert!(
            matches!(
                result.status,
                RegistrationStatus::Installed | RegistrationStatus::Created
            ),
            "Expected installed, got: {:?}",
            result.status
        );

        let config_path = project_root.join(".mcp.json");
        assert!(config_path.exists());

        let config: Value =
            serde_json::from_str(&std::fs::read_to_string(&config_path).unwrap()).unwrap();
        assert!(config["mcpServers"]["hief"].is_object());
        assert_eq!(config["mcpServers"]["hief"]["args"][0], "serve");
    }
}
