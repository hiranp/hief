use clap::Parser;
use hief::cli::{Cli, Commands, InstallArgs};
use hief::cli::commands::build_install_preview;
use hief::config::{Config, RouterConfig};
use hief::router::{select_lane, OperationRequest, ProtocolLane};
use tempfile::TempDir;

fn test_config() -> Config {
    Config {
        router: RouterConfig {
            router_path: ".hief/router.toml".to_string(),
            default_lane: ProtocolLane::Cli,
            token_pressure_threshold: 512,
        },
        ..Config::default()
    }
}

#[test]
fn test_local_deterministic_operations_route_to_cli_lane() {
    let config = test_config();
    let decision = select_lane(&OperationRequest::local("index-status", 64), &config.router);

    assert_eq!(decision.lane, ProtocolLane::Cli);
    assert_eq!(decision.reason, "operation is local and deterministic");
}

#[test]
fn test_remote_operations_route_to_mcp_lane() {
    let config = test_config();
    let decision = select_lane(&OperationRequest::remote("mcp-install", 128), &config.router);

    assert_eq!(decision.lane, ProtocolLane::Mcp);
    assert_eq!(decision.reason, "operation requires remote or authenticated execution");
}

#[test]
fn test_token_pressure_operations_route_to_progressive_mcp_lane() {
    let config = test_config();
    let decision = select_lane(&OperationRequest::local("large-search", 1024), &config.router);

    assert_eq!(decision.lane, ProtocolLane::ProgressiveMcp);
    assert!(decision.reason.contains("estimated token pressure 1024 exceeds threshold 512"));
}

#[test]
fn test_install_command_parses_platform_and_default_dry_run() {
    let cli = Cli::parse_from(["hief", "install", "--platform", "claude-desktop"]);

    match cli.command {
        Commands::Install(InstallArgs { platform, dry_run }) => {
            assert_eq!(platform, "claude-desktop");
            assert!(dry_run);
        }
        _ => panic!("expected install command"),
    }
}

#[test]
fn test_install_preview_for_claude_desktop_is_deterministic() {
    let tempdir = TempDir::new().expect("tempdir");
    let preview = build_install_preview(&test_config(), tempdir.path(), "claude-desktop", true)
        .expect("valid preview");

    assert_eq!(preview.platform, "claude-desktop");
    assert!(preview.dry_run);
    assert_eq!(preview.lane, "mcp");
    assert!(preview.config_block.contains("[mcp_servers.hief]"));
    assert!(preview.config_block.contains("platform = \"claude-desktop\""));
}

#[test]
fn test_unknown_platform_returns_typed_validation_error() {
    let tempdir = TempDir::new().expect("tempdir");
    let err = build_install_preview(&test_config(), tempdir.path(), "unknown-platform", true)
        .expect_err("unknown platform should fail");

    let message = err.to_string();
    assert!(message.contains("invalid tool params for install"));
    assert!(message.contains("platform"));
    assert!(message.contains("unknown-platform"));
}

#[test]
fn test_install_command_accepts_dry_run_false_but_marks_execution_deferred() {
    let cli = Cli::parse_from([
        "hief",
        "install",
        "--platform",
        "claude-desktop",
        "--dry-run=false",
    ]);

    let (platform, dry_run) = match cli.command {
        Commands::Install(InstallArgs { platform, dry_run }) => (platform, dry_run),
        _ => panic!("expected install command"),
    };

    let tempdir = TempDir::new().expect("tempdir");
    let preview = build_install_preview(&test_config(), tempdir.path(), &platform, dry_run)
        .expect("preview");

    assert!(!preview.dry_run);
    assert!(preview.deferred);
    assert!(preview.message.contains("deferred"));
}