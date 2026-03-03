//! Tests for configuration loading and defaults.

use std::io::Write;
use tempfile::NamedTempFile;

// We test the config module via the public binary interface,
// but since config is internal, we test it as an integration test
// using the same logic.

#[test]
fn test_default_config_values() {
    // Verify default TOML parses correctly
    let toml_str = r#"
[hief]
version = "0.1.0"
"#;
    let config: toml::Value = toml::from_str(toml_str).unwrap();
    assert_eq!(config["hief"]["version"].as_str().unwrap(), "0.1.0");
}

#[test]
fn test_full_config_parse() {
    let toml_str = r#"
[hief]
version = "0.1.0"

[index]
chunk_strategy = "ast"
max_chunk_tokens = 1024
languages = ["rust", "python"]

[graph]
require_approval = false

[eval]
golden_set_path = ".hief/golden/"
min_score = 0.90
fail_on_regression = true

[serve]
transport = "http"
host = "0.0.0.0"
port = 8080
"#;
    let config: toml::Value = toml::from_str(toml_str).unwrap();
    assert_eq!(config["index"]["max_chunk_tokens"].as_integer().unwrap(), 1024);
    assert_eq!(config["graph"]["require_approval"].as_bool().unwrap(), false);
    assert_eq!(config["eval"]["min_score"].as_float().unwrap(), 0.90);
    assert_eq!(config["serve"]["port"].as_integer().unwrap(), 8080);
}

#[test]
fn test_minimal_config_parse() {
    // Only the required [hief] section
    let toml_str = r#"
[hief]
version = "0.1.0"
"#;
    let config: toml::Value = toml::from_str(toml_str).unwrap();
    assert!(config.get("index").is_none()); // defaults not in TOML
}

#[test]
fn test_invalid_toml_rejected() {
    let bad_toml = "this is { not valid toml";
    let result: Result<toml::Value, _> = toml::from_str(bad_toml);
    assert!(result.is_err());
}

#[test]
fn test_config_serialization_roundtrip() {
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Serialize, Deserialize, PartialEq)]
    struct TestConfig {
        version: String,
        max_tokens: usize,
    }

    let original = TestConfig {
        version: "0.1.0".to_string(),
        max_tokens: 512,
    };

    let serialized = toml::to_string(&original).unwrap();
    let deserialized: TestConfig = toml::from_str(&serialized).unwrap();
    assert_eq!(original, deserialized);
}
