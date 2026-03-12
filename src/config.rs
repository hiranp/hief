//! Configuration parsing for hief.toml.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::errors::{HiefError, Result};

/// Top-level configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub hief: HiefConfig,
    #[serde(default)]
    pub index: IndexConfig,
    #[serde(default)]
    pub graph: GraphConfig,
    #[serde(default)]
    pub eval: EvalConfig,
    #[serde(default)]
    pub serve: ServeConfig,
    #[serde(default)]
    pub docs: DocsConfig,
    #[serde(default)]
    pub skills: SkillsConfig,
    #[serde(default)]
    pub vectors: crate::index::vectors::VectorConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HiefConfig {
    #[serde(default = "default_version")]
    pub version: String,
}

fn default_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexConfig {
    #[serde(default = "default_chunk_strategy")]
    pub chunk_strategy: String,
    #[serde(default = "default_max_chunk_tokens")]
    pub max_chunk_tokens: usize,
    #[serde(default = "default_languages")]
    pub languages: Vec<String>,
    #[cfg(feature = "embeddings")]
    pub embeddings: Option<EmbeddingsConfig>,
}

impl Default for IndexConfig {
    fn default() -> Self {
        Self {
            chunk_strategy: default_chunk_strategy(),
            max_chunk_tokens: default_max_chunk_tokens(),
            languages: default_languages(),
            #[cfg(feature = "embeddings")]
            embeddings: None,
        }
    }
}

fn default_chunk_strategy() -> String {
    "ast".to_string()
}

fn default_max_chunk_tokens() -> usize {
    512
}

fn default_languages() -> Vec<String> {
    vec![
        "rust".to_string(),
        "python".to_string(),
        "typescript".to_string(),
    ]
}

#[cfg(feature = "embeddings")]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingsConfig {
    pub provider: String,
    pub model: String,
    pub base_url: String,
    #[serde(default = "default_dimensions")]
    pub dimensions: usize,
}

#[cfg(feature = "embeddings")]
fn default_dimensions() -> usize {
    768
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphConfig {
    #[serde(default = "default_require_approval")]
    pub require_approval: bool,
    /// Hours before an `in_progress` intent is considered stale and eligible
    /// for automatic recovery (reset to `approved` so another agent can pick
    /// it up). Set to 0 to disable stale detection. Default: 48.
    #[serde(default = "default_stale_timeout_hours")]
    pub stale_timeout_hours: u64,
}

impl Default for GraphConfig {
    fn default() -> Self {
        Self {
            require_approval: default_require_approval(),
            stale_timeout_hours: default_stale_timeout_hours(),
        }
    }
}

fn default_require_approval() -> bool {
    true
}

fn default_stale_timeout_hours() -> u64 {
    48
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalConfig {
    #[serde(default = "default_golden_set_path")]
    pub golden_set_path: String,
    #[serde(default = "default_min_score")]
    pub min_score: f64,
    #[serde(default = "default_fail_on_regression")]
    pub fail_on_regression: bool,
}

impl Default for EvalConfig {
    fn default() -> Self {
        Self {
            golden_set_path: default_golden_set_path(),
            min_score: default_min_score(),
            fail_on_regression: default_fail_on_regression(),
        }
    }
}

fn default_golden_set_path() -> String {
    ".hief/golden/".to_string()
}

fn default_min_score() -> f64 {
    0.85
}

fn default_fail_on_regression() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServeConfig {
    #[serde(default = "default_transport")]
    pub transport: String,
    #[serde(default = "default_host")]
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
}

impl Default for ServeConfig {
    fn default() -> Self {
        Self {
            transport: default_transport(),
            host: default_host(),
            port: default_port(),
        }
    }
}

fn default_transport() -> String {
    "stdio".to_string()
}

fn default_host() -> String {
    "127.0.0.1".to_string()
}

fn default_port() -> u16 {
    3100
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocsConfig {
    #[serde(default = "default_docs_path")]
    pub docs_path: String,
    #[serde(default = "default_specs_path")]
    pub specs_path: String,
    #[serde(default = "default_harness_path")]
    pub harness_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillsConfig {
    /// Path where skill files live (relative to project root)
    #[serde(default = "default_skills_path")]
    pub skills_path: String,
}

impl Default for SkillsConfig {
    fn default() -> Self {
        Self {
            skills_path: default_skills_path(),
        }
    }
}

fn default_skills_path() -> String {
    ".hief/skills/".to_string()
}

impl Default for DocsConfig {
    fn default() -> Self {
        Self {
            docs_path: default_docs_path(),
            specs_path: default_specs_path(),
            harness_path: default_harness_path(),
        }
    }
}

fn default_docs_path() -> String {
    "docs".to_string()
}

fn default_specs_path() -> String {
    "docs/specs".to_string()
}

fn default_harness_path() -> String {
    "docs/harness".to_string()
}

impl Config {
    /// Load config from a file path. Falls back to defaults if file doesn't exist.
    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }

        let content = std::fs::read_to_string(path)
            .map_err(|e| HiefError::Config(format!("failed to read {}: {}", path.display(), e)))?;

        let config: Config = toml::from_str(&content)
            .map_err(|e| HiefError::Config(format!("failed to parse {}: {}", path.display(), e)))?;

        Ok(config)
    }

    /// Write default config to a file.
    pub fn write_default(path: &Path) -> Result<()> {
        let config = Self::default();
        let content = toml::to_string_pretty(&config)
            .map_err(|e| HiefError::Config(format!("failed to serialize config: {}", e)))?;
        std::fs::write(path, content)?;
        Ok(())
    }

    /// Returns the .hief directory path relative to project root.
    pub fn hief_dir(project_root: &Path) -> PathBuf {
        project_root.join(".hief")
    }

    /// Returns the database file path.
    pub fn db_path(project_root: &Path) -> PathBuf {
        Self::hief_dir(project_root).join("hief.db")
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            hief: HiefConfig {
                version: default_version(),
            },
            index: IndexConfig::default(),
            graph: GraphConfig::default(),
            eval: EvalConfig::default(),
            serve: ServeConfig::default(),
            docs: DocsConfig::default(),
            skills: SkillsConfig::default(),
            vectors: crate::index::vectors::VectorConfig::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.index.chunk_strategy, "ast");
        assert_eq!(config.index.max_chunk_tokens, 512);
        assert_eq!(config.index.languages.len(), 3);
        assert!(config.graph.require_approval);
        assert_eq!(config.eval.min_score, 0.85);
        assert!(config.eval.fail_on_regression);
        assert_eq!(config.serve.transport, "stdio");
        assert_eq!(config.serve.host, "127.0.0.1");
        assert_eq!(config.serve.port, 3100);
        // skills path default
        assert_eq!(config.skills.skills_path, ".hief/skills/");
    }

    #[test]
    fn test_load_nonexistent_file_returns_defaults() {
        let config = Config::load(Path::new("/nonexistent/hief.toml")).unwrap();
        assert_eq!(config.index.max_chunk_tokens, 512);
        assert_eq!(config.serve.port, 3100);
    }

    #[test]
    fn test_load_valid_toml() {
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        write!(
            tmp,
            r#"
[hief]
version = "0.1.0"

[index]
max_chunk_tokens = 1024
languages = ["rust"]

[serve]
port = 8080
"#
        )
        .unwrap();

        let config = Config::load(tmp.path()).unwrap();
        assert_eq!(config.index.max_chunk_tokens, 1024);
        assert_eq!(config.index.languages, vec!["rust"]);
        assert_eq!(config.serve.port, 8080);
        // default skills path remains unchanged when not specified
        assert_eq!(config.skills.skills_path, ".hief/skills/");
    }

    #[test]
    fn test_load_invalid_toml_returns_error() {
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        write!(tmp, "this is {{ not valid toml").unwrap();

        let result = Config::load(tmp.path());
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("config error"), "got: {err}");
    }

    #[test]
    fn test_write_default_and_reload() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hief.toml");

        Config::write_default(&path).unwrap();
        assert!(path.exists());

        let config = Config::load(&path).unwrap();
        assert_eq!(config.index.chunk_strategy, "ast");
        assert_eq!(config.serve.transport, "stdio");
    }

    #[test]
    fn test_db_path() {
        let root = Path::new("/tmp/project");
        assert_eq!(
            Config::db_path(root),
            PathBuf::from("/tmp/project/.hief/hief.db")
        );
    }

    #[test]
    fn test_hief_dir() {
        let root = Path::new("/tmp/project");
        assert_eq!(Config::hief_dir(root), PathBuf::from("/tmp/project/.hief"));
    }

    #[test]
    fn test_config_serialization_roundtrip() {
        let original = Config::default();
        let toml_str = toml::to_string_pretty(&original).unwrap();
        let deserialized: Config = toml::from_str(&toml_str).unwrap();
        assert_eq!(
            deserialized.index.max_chunk_tokens,
            original.index.max_chunk_tokens
        );
        assert_eq!(deserialized.serve.port, original.serve.port);
    }
}
