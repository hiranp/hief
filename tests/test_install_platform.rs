//! Integration tests for the `hief install --platform` write path (PRO-04).
//!
//! Verifies that:
//! 1. Dry-run mode returns a config preview without writing any files.
//! 2. Non-dry-run mode delegates to `mcp_install` and actually writes the
//!    platform config file (idempotent, project-scope).
//! 3. Invalid platform names return a typed constraint error.

use hief::cli::commands::{InstallPlatform, build_install_preview, install_platform};
use hief::config::Config;

fn default_config() -> Config {
    Config::default()
}

// ---------------------------------------------------------------------------
// InstallPlatform enum parsing
// ---------------------------------------------------------------------------

#[test]
fn test_install_platform_parse_valid_names() {
    assert_eq!(
        InstallPlatform::parse("claude-desktop").expect("parse claude-desktop"),
        InstallPlatform::ClaudeDesktop
    );
    assert_eq!(
        InstallPlatform::parse("cursor").expect("parse cursor"),
        InstallPlatform::Cursor
    );
    assert_eq!(
        InstallPlatform::parse("zed").expect("parse zed"),
        InstallPlatform::Zed
    );
    assert_eq!(
        InstallPlatform::parse("custom").expect("parse custom"),
        InstallPlatform::Custom
    );
    // Case insensitive
    assert_eq!(
        InstallPlatform::parse("CURSOR").expect("parse uppercase"),
        InstallPlatform::Cursor
    );
}

#[test]
fn test_install_platform_parse_invalid_name_returns_constraint_error() {
    let err = InstallPlatform::parse("nonexistent").expect_err("should fail");
    let msg = err.to_string();
    // Error message should mention the constraint but NOT echo the raw input
    // in a way that could confuse users about valid options.
    assert!(
        msg.contains("must be one of") || msg.contains("platform"),
        "error should explain valid values: {msg}"
    );
}

// ---------------------------------------------------------------------------
// Dry-run preview
// ---------------------------------------------------------------------------

#[test]
fn test_build_install_preview_dry_run_returns_preview_not_deferred() {
    let dir = tempfile::tempdir().expect("tempdir");
    let config = default_config();

    let preview =
        build_install_preview(&config, dir.path(), "cursor", true).expect("build preview");

    assert_eq!(preview.platform, "cursor");
    assert!(preview.dry_run);
    assert!(!preview.deferred, "dry-run preview should not be deferred");
    assert!(
        preview.config_block.contains("hief"),
        "config block should mention hief"
    );
    assert!(
        preview.message.contains("dry-run"),
        "message should mention dry-run: {}",
        preview.message
    );
}

#[test]
fn test_build_install_preview_non_dry_run_is_deferred_in_preview() {
    let dir = tempfile::tempdir().expect("tempdir");
    let config = default_config();

    let preview =
        build_install_preview(&config, dir.path(), "cursor", false).expect("build preview");

    assert!(!preview.dry_run);
    assert!(
        preview.deferred,
        "non-dry-run preview should mark deferred=true until write happens"
    );
}

#[test]
fn test_install_platform_dry_run_writes_no_files() {
    let dir = tempfile::tempdir().expect("tempdir");
    let config = default_config();

    install_platform(&config, dir.path(), "cursor", true, false).expect("dry run should succeed");

    // No config files should have been written
    let cursor_config = dir.path().join(".cursor").join("mcp.json");
    assert!(
        !cursor_config.exists(),
        "dry-run must not create .cursor/mcp.json"
    );
}

// ---------------------------------------------------------------------------
// Real write path (PRO-04)
// ---------------------------------------------------------------------------

#[test]
fn test_install_platform_real_write_creates_cursor_config() {
    let dir = tempfile::tempdir().expect("tempdir");
    let config = default_config();

    install_platform(&config, dir.path(), "cursor", false, false)
        .expect("install platform should succeed");

    // cursor uses project-scope → .cursor/mcp.json
    let cursor_config = dir.path().join(".cursor").join("mcp.json");
    assert!(
        cursor_config.exists(),
        "non-dry-run install should create .cursor/mcp.json"
    );

    let content = std::fs::read_to_string(&cursor_config).expect("read config");
    let json: serde_json::Value = serde_json::from_str(&content).expect("valid JSON config");

    // Verify HIEF is registered in the mcpServers object
    assert!(
        json["mcpServers"]["hief"].is_object(),
        "hief entry should be present in mcpServers: {json}"
    );
    assert_eq!(
        json["mcpServers"]["hief"]["args"][0], "serve",
        "args should start with 'serve'"
    );
}

#[test]
fn test_install_platform_real_write_is_idempotent() {
    let dir = tempfile::tempdir().expect("tempdir");
    let config = default_config();

    // First install
    install_platform(&config, dir.path(), "cursor", false, false).expect("first install");

    // Second install — must not fail and must not duplicate the entry
    install_platform(&config, dir.path(), "cursor", false, false)
        .expect("second install should be idempotent");

    let cursor_config = dir.path().join(".cursor").join("mcp.json");
    let content = std::fs::read_to_string(&cursor_config).expect("read config");
    let json: serde_json::Value = serde_json::from_str(&content).expect("valid JSON");

    // mcpServers.hief should exist exactly once (not duplicated as an array)
    assert!(json["mcpServers"]["hief"].is_object());
}

#[test]
fn test_install_platform_invalid_platform_returns_error() {
    let dir = tempfile::tempdir().expect("tempdir");
    let config = default_config();

    let err = install_platform(&config, dir.path(), "unknown-platform", false, false)
        .expect_err("invalid platform should fail");

    let msg = err.to_string();
    assert!(
        msg.contains("platform") || msg.contains("must be one of"),
        "error should reference platform constraint: {msg}"
    );
}

#[test]
fn test_install_platform_claude_desktop_dry_run_no_home_writes() {
    // Claude Desktop writes to the global home dir — dry-run must NEVER touch it.
    let dir = tempfile::tempdir().expect("tempdir");
    let config = default_config();

    // This only tests that the call succeeds without writing — the global path
    // is $HOME/.../Claude/claude_desktop_config.json which we can't tempdir-scope.
    // The assertion is purely that no error is returned and no panic occurs.
    let result = install_platform(&config, dir.path(), "claude-desktop", true, false);
    assert!(
        result.is_ok(),
        "claude-desktop dry-run should not error: {:?}",
        result
    );
}
