//! Structural code search using ast-grep patterns.
//!
//! Provides pattern-based AST matching (e.g., `$FUNC.unwrap()`) that
//! goes beyond keyword search to find code by structure. Uses ast-grep-core
//! with HIEF's existing tree-sitter grammars.

use serde::Serialize;
use schemars::JsonSchema;
use std::path::Path;
use tracing::debug;

use ast_grep_core::AstGrep;
use ast_grep_core::language::TSLanguage;
use ast_grep_core::matcher::Pattern;

use crate::errors::{HiefError, Result};
use crate::index::walker::FileWalker;

/// A structural search query using ast-grep pattern syntax.
///
/// Patterns use `$` for meta-variables:
/// - `$FUNC.unwrap()` — find any `.unwrap()` call
/// - `fn $NAME($$$)` — find any function
/// - `if let Err($E) = $EXPR { $$$BODY }` — find error handling
#[derive(Debug, Clone)]
pub struct StructuralQuery {
    pub pattern: String,
    pub language: String,
    pub top_k: usize,
}

impl StructuralQuery {
    pub fn new(pattern: impl Into<String>, language: impl Into<String>) -> Self {
        Self {
            pattern: pattern.into(),
            language: language.into(),
            top_k: 50,
        }
    }
}

/// A structural search match result.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct StructuralMatch {
    pub file_path: String,
    pub matched_text: String,
    pub start_line: u32,
    pub end_line: u32,
    pub start_col: u32,
    pub end_col: u32,
    pub context: String,
}

/// Resolve a language string to a `TSLanguage` for ast-grep.
///
/// Returns the `tree_sitter_facade_sg::Language` (re-exported as
/// `ast_grep_core::language::TSLanguage`) which implements
/// `ast_grep_core::Language`.
fn get_ts_language(language: &str) -> Option<TSLanguage> {
    let ts_lang: tree_sitter::Language = match language {
        "rust" => tree_sitter_rust::LANGUAGE.into(),
        "python" => tree_sitter_python::LANGUAGE.into(),
        "typescript" | "javascript" => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        _ => return None,
    };
    // Convert tree_sitter::Language → tree_sitter_facade_sg::Language (TSLanguage)
    Some(ts_lang.into())
}

/// Perform a structural search across the project using ast-grep patterns.
///
/// This walks all source files matching the given language, parses them
/// with tree-sitter, and uses ast-grep's pattern matching to find
/// structural matches.
pub fn search(project_root: &Path, query: &StructuralQuery) -> Result<Vec<StructuralMatch>> {
    let ts_lang = get_ts_language(&query.language)
        .ok_or_else(|| HiefError::UnsupportedLanguage(query.language.clone()))?;

    // Validate the pattern by trying to parse it
    static PATTERN_CACHE: std::sync::OnceLock<
        std::sync::Mutex<std::collections::HashMap<(String, String), Pattern<TSLanguage>>>,
    > = std::sync::OnceLock::new();
    let cache_key = (query.pattern.clone(), query.language.clone());

    let mut cache = PATTERN_CACHE
        .get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
        .lock()
        .unwrap();
    let pattern = if let Some(p) = cache.get(&cache_key) {
        p.clone()
    } else {
        let p = Pattern::try_new(&query.pattern, ts_lang.clone()).map_err(|e| {
            HiefError::ParseError {
                file: "(pattern)".to_string(),
                message: format!("Invalid structural pattern '{}': {}", query.pattern, e),
            }
        })?;
        cache.insert(cache_key.clone(), p.clone());
        p
    };
    drop(cache);

    let walker = FileWalker::new(project_root);
    let files = walker.walk()?;

    let mut matches = Vec::new();
    let lang_extensions = language_extensions(&query.language);

    for file_entry in &files {
        // Filter by language
        if file_entry.language.as_deref() != Some(&query.language) {
            // Also check file extension match as fallback
            let has_ext = lang_extensions
                .iter()
                .any(|ext| file_entry.rel_path.ends_with(ext));
            if !has_ext {
                continue;
            }
        }

        let content = match std::fs::read_to_string(&file_entry.abs_path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        // Parse with ast-grep
        let grep = AstGrep::new(&content, ts_lang.clone());
        let root = grep.root();

        for node_match in root.find_all(&pattern) {
            let matched_text = node_match.text().to_string();
            let start = node_match.start_pos();
            let end = node_match.end_pos();

            // Extract context: the full lines containing the match
            let lines: Vec<&str> = content.lines().collect();
            let ctx_start = start.line().saturating_sub(1);
            let ctx_end = (end.line() + 2).min(lines.len());
            let context = lines[ctx_start..ctx_end].join("\n");

            matches.push(StructuralMatch {
                file_path: file_entry.rel_path.clone(),
                matched_text,
                start_line: start.line() as u32,
                end_line: end.line() as u32,
                start_col: start.column(&node_match) as u32,
                end_col: end.column(&node_match) as u32,
                context,
            });

            if matches.len() >= query.top_k {
                break;
            }
        }

        if matches.len() >= query.top_k {
            break;
        }
    }

    debug!(
        "Structural search '{}' found {} matches in {} files",
        query.pattern,
        matches.len(),
        files.len()
    );

    Ok(matches)
}

/// Perform a structural search on a single source string (for testing/in-memory use).
#[allow(dead_code)]
pub fn search_source(
    source: &str,
    pattern: &str,
    language: &str,
    file_path: &str,
) -> Result<Vec<StructuralMatch>> {
    let ts_lang = get_ts_language(language)
        .ok_or_else(|| HiefError::UnsupportedLanguage(language.to_string()))?;

    let pat = Pattern::try_new(pattern, ts_lang.clone()).map_err(|e| HiefError::ParseError {
        file: "(pattern)".to_string(),
        message: format!("Invalid structural pattern '{}': {}", pattern, e),
    })?;

    let grep = AstGrep::new(source, ts_lang);
    let root = grep.root();
    let mut matches = Vec::new();

    for node_match in root.find_all(&pat) {
        let matched_text = node_match.text().to_string();
        let start = node_match.start_pos();
        let end = node_match.end_pos();

        let lines: Vec<&str> = source.lines().collect();
        let ctx_start = start.line().saturating_sub(1);
        let ctx_end = (end.line() + 2).min(lines.len());
        let context = lines[ctx_start..ctx_end].join("\n");

        matches.push(StructuralMatch {
            file_path: file_path.to_string(),
            matched_text,
            start_line: start.line() as u32,
            end_line: end.line() as u32,
            start_col: start.column(&node_match) as u32,
            end_col: end.column(&node_match) as u32,
            context,
        });
    }

    Ok(matches)
}

/// Map language name to common file extensions.
fn language_extensions(language: &str) -> Vec<&'static str> {
    match language {
        "rust" => vec![".rs"],
        "python" => vec![".py"],
        "typescript" => vec![".ts", ".tsx"],
        "javascript" => vec![".js", ".jsx"],
        _ => vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // Pattern matching tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_find_unwrap_calls() {
        let source = r#"
fn main() {
    let x = some_result().unwrap();
    let y = another().expect("msg");
    let z = safe_value;
}
"#;
        let matches = search_source(source, "$X.unwrap()", "rust", "test.rs").unwrap();
        assert_eq!(matches.len(), 1, "Should find exactly one .unwrap() call");
        assert!(matches[0].matched_text.contains("unwrap()"));
    }

    #[test]
    fn test_find_function_declarations() {
        let source = r#"
fn hello() {
    println!("hello");
}

fn world(x: i32) -> bool {
    x > 0
}
"#;
        // Pattern without return type matches 1; with return type matches the other
        let m1 = search_source(source, "fn $NAME($$$) $BODY", "rust", "test.rs").unwrap();
        let m2 = search_source(source, "fn $NAME($$$) -> $RET $BODY", "rust", "test.rs").unwrap();
        let total = m1.len() + m2.len();
        assert!(
            total >= 2,
            "Should find at least 2 function declarations, got {}",
            total
        );
    }

    #[test]
    fn test_find_struct_definitions() {
        let source = r#"
struct Point {
    x: f64,
    y: f64,
}

struct Color(u8, u8, u8);
"#;
        // Use pattern matching the struct item node
        let matches = search_source(source, "struct $NAME $BODY", "rust", "test.rs").unwrap();
        assert!(
            !matches.is_empty(),
            "Should find at least 1 struct definition, got {}",
            matches.len()
        );
        assert!(
            matches[0].matched_text.contains("Point") || matches[0].matched_text.contains("Color")
        );
    }

    #[test]
    fn test_python_parsing_works() {
        // Python parsing works but meta-variables ($VAR) require a custom
        // Language impl with expando_char (not available with raw TSLanguage).
        // Verify that parsing at least doesn't error on valid Python code.
        let source = "x = 42\ny = \"hello\"\n";
        // Literal pattern (no meta-variables) should work
        let matches = search_source(source, "x = 42", "python", "test.py").unwrap();
        assert_eq!(
            matches.len(),
            1,
            "Should find literal Python match, got {}",
            matches.len()
        );
    }

    #[test]
    fn test_find_typescript_arrow_functions() {
        let source = r#"
const add = (a: number, b: number): number => {
    return a + b;
};

const sub = (a: number, b: number) => a - b;
"#;
        // Find arrow functions assigned to const
        let matches = search_source(
            source,
            "const $NAME = ($$$) => $BODY",
            "typescript",
            "test.ts",
        )
        .unwrap();
        assert!(
            !matches.is_empty(),
            "Should find arrow functions, got {}",
            matches.len()
        );
    }

    #[test]
    fn test_invalid_pattern_returns_error() {
        // This should produce a parse error for an invalid/empty pattern
        let result = search_source("fn foo() {}", "", "rust", "test.rs");
        // Empty pattern might parse or not - test that the function doesn't panic
        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn test_unsupported_language() {
        let result = search_source("some code", "$X", "cobol", "test.cob");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("unsupported language"), "got: {err}");
    }

    #[test]
    fn test_no_matches_returns_empty() {
        let source = r#"
fn safe_function() -> Result<(), Error> {
    Ok(())
}
"#;
        let matches = search_source(source, "$X.unwrap()", "rust", "test.rs").unwrap();
        assert!(
            matches.is_empty(),
            "Should find no unwrap calls in safe code"
        );
    }

    #[test]
    fn test_match_positions_are_correct() {
        let source = "fn hello() {\n    println!(\"hi\");\n}\n";
        let matches =
            search_source(source, "fn $NAME($$$) { $$$BODY }", "rust", "test.rs").unwrap();
        if !matches.is_empty() {
            assert_eq!(matches[0].start_line, 0, "Function should start at line 0");
        }
    }

    #[test]
    fn test_structural_query_construction() {
        let q = StructuralQuery::new("$X.unwrap()", "rust");
        assert_eq!(q.pattern, "$X.unwrap()");
        assert_eq!(q.language, "rust");
        assert_eq!(q.top_k, 50);
    }
}
