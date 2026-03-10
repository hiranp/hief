//! Domain error types for HIEF.

use thiserror::Error;

#[derive(Error, Debug)]
pub enum HiefError {
    // Database errors
    #[error("database error: {0}")]
    Database(#[from] libsql::Error),

    #[error("migration failed: {0}")]
    Migration(String),

    // Config errors
    #[error("config error: {0}")]
    Config(String),

    #[allow(dead_code)]
    #[error("config file not found: {0}")]
    ConfigNotFound(String),

    // Index errors
    #[error("unsupported language: {0}")]
    UnsupportedLanguage(String),

    #[error("parse error in {file}: {message}")]
    ParseError { file: String, message: String },

    #[allow(dead_code)]
    #[error("index not initialized — run `hief init` first")]
    IndexNotInitialized,

    // Graph errors
    #[error("intent not found: {0}")]
    IntentNotFound(String),

    #[error("invalid status transition: {from} → {to}")]
    InvalidTransition { from: String, to: String },

    #[allow(dead_code)]
    #[error("cycle detected in intent graph: {0}")]
    CycleDetected(String),

    #[error("duplicate edge: {from} → {to} ({kind})")]
    DuplicateEdge {
        from: String,
        to: String,
        kind: String,
    },

    #[error("ambiguous intent ID prefix '{prefix}' matches {count} intents: {matches_str}", count = matches.len(), matches_str = matches.join(", "))]
    AmbiguousId {
        prefix: String,
        matches: Vec<String>,
    },

    // Eval errors
    #[error("golden set not found: {0}")]
    GoldenSetNotFound(String),

    #[error("golden set parse error: {0}")]
    GoldenSetParse(String),

    #[allow(dead_code)]
    #[error("evaluation failed: {0}")]
    EvalFailed(String),

    // Security errors
    #[error("security violation: {0}")]
    SecurityViolation(String),

    #[error("path traversal detected: {0}")]
    PathTraversal(String),

    // IO errors
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, HiefError>;
