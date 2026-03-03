//! AST-aware code chunking using tree-sitter.

use serde::Serialize;
use tracing::debug;

/// A chunk of source code extracted from a file.
#[derive(Debug, Clone, Serialize)]
pub struct Chunk {
    pub file_path: String,
    pub symbol_name: Option<String>,
    pub symbol_kind: Option<String>,
    pub parent_scope: Option<String>,
    pub language: String,
    pub content: String,
    pub start_line: u32,
    pub end_line: u32,
    pub content_hash: String,
}

/// Chunker that produces code chunks from source files.
pub struct Chunker {
    max_chunk_tokens: usize,
}

impl Chunker {
    pub fn new(max_chunk_tokens: usize) -> Self {
        Self { max_chunk_tokens }
    }

    /// Chunk a source file into meaningful code segments.
    ///
    /// For languages with tree-sitter support (Rust, Python, TypeScript),
    /// this produces AST-aware chunks with symbol metadata.
    /// For other languages, falls back to line-based chunking.
    pub fn chunk(&self, source: &str, language: &str, file_path: &str) -> Vec<Chunk> {
        match language {
            "rust" => self.chunk_with_treesitter(source, language, file_path, get_rust_queries()),
            "python" => {
                self.chunk_with_treesitter(source, language, file_path, get_python_queries())
            }
            "typescript" | "javascript" => {
                self.chunk_with_treesitter(source, language, file_path, get_ts_queries())
            }
            _ => self.chunk_lines(source, language, file_path),
        }
    }

    /// AST-aware chunking using tree-sitter.
    fn chunk_with_treesitter(
        &self,
        source: &str,
        language: &str,
        file_path: &str,
        chunkable_kinds: &[&str],
    ) -> Vec<Chunk> {
        let Some(ts_language) = get_tree_sitter_language(language) else {
            return self.chunk_lines(source, language, file_path);
        };

        let mut parser = tree_sitter::Parser::new();
        if parser.set_language(&ts_language).is_err() {
            return self.chunk_lines(source, language, file_path);
        }

        let Some(tree) = parser.parse(source, None) else {
            return self.chunk_lines(source, language, file_path);
        };

        let root = tree.root_node();
        let mut chunks = Vec::new();
        let mut preamble_end = 0u32;

        // Walk top-level nodes
        let mut cursor = root.walk();
        for child in root.children(&mut cursor) {
            let kind = child.kind();
            let start_line = child.start_position().row as u32;
            let end_line = child.end_position().row as u32;
            let text = &source[child.byte_range()];

            if chunkable_kinds.contains(&kind) {
                // This is a symbol node — extract it
                let (symbol_name, symbol_kind) = extract_symbol_info(&child, source);

                // If the chunk is too large, try to split at child boundaries
                let token_estimate = text.split_whitespace().count();
                if token_estimate > self.max_chunk_tokens {
                    // Split into sub-chunks (e.g., individual methods in an impl block)
                    let sub_chunks = self.split_large_node(
                        &child,
                        source,
                        language,
                        file_path,
                        symbol_name.as_deref(),
                        chunkable_kinds,
                    );
                    if !sub_chunks.is_empty() {
                        chunks.extend(sub_chunks);
                        continue;
                    }
                }

                let content_hash = blake3::hash(text.as_bytes()).to_hex().to_string();
                chunks.push(Chunk {
                    file_path: file_path.to_string(),
                    symbol_name,
                    symbol_kind: Some(symbol_kind),
                    parent_scope: None,
                    language: language.to_string(),
                    content: text.to_string(),
                    start_line,
                    end_line,
                    content_hash,
                });
            } else {
                // Track preamble extent (imports, etc.)
                preamble_end = end_line + 1;
            }
        }

        // Create a preamble chunk for non-symbol top-level code
        if preamble_end > 0 {
            let preamble_lines: Vec<&str> = source.lines().take(preamble_end as usize).collect();
            let preamble_text = preamble_lines.join("\n");
            if !preamble_text.trim().is_empty() {
                let content_hash = blake3::hash(preamble_text.as_bytes()).to_hex().to_string();
                chunks.insert(
                    0,
                    Chunk {
                        file_path: file_path.to_string(),
                        symbol_name: Some("_preamble".to_string()),
                        symbol_kind: Some("preamble".to_string()),
                        parent_scope: None,
                        language: language.to_string(),
                        content: preamble_text,
                        start_line: 0,
                        end_line: preamble_end.saturating_sub(1),
                        content_hash,
                    },
                );
            }
        }

        if chunks.is_empty() {
            // Fallback: whole file as one chunk
            return self.chunk_lines(source, language, file_path);
        }

        debug!(
            "Chunked {} into {} chunks (AST)",
            file_path,
            chunks.len()
        );
        chunks
    }

    /// Split a large AST node into sub-chunks at child boundaries.
    fn split_large_node(
        &self,
        node: &tree_sitter::Node,
        source: &str,
        language: &str,
        file_path: &str,
        parent_name: Option<&str>,
        chunkable_kinds: &[&str],
    ) -> Vec<Chunk> {
        let mut sub_chunks = Vec::new();
        let mut cursor = node.walk();

        for child in node.children(&mut cursor) {
            let kind = child.kind();
            if !chunkable_kinds.contains(&kind) {
                // Look one level deeper for methods/functions inside blocks
                let mut inner_cursor = child.walk();
                for inner_child in child.children(&mut inner_cursor) {
                    let inner_kind = inner_child.kind();
                    if chunkable_kinds.contains(&inner_kind) {
                        let text = &source[inner_child.byte_range()];
                        let (sym_name, sym_kind) = extract_symbol_info(&inner_child, source);
                        let scope = match (parent_name, sym_name.as_deref()) {
                            (Some(p), Some(s)) => Some(format!("{}/{}", p, s)),
                            (Some(p), None) => Some(p.to_string()),
                            _ => None,
                        };
                        let content_hash = blake3::hash(text.as_bytes()).to_hex().to_string();
                        sub_chunks.push(Chunk {
                            file_path: file_path.to_string(),
                            symbol_name: sym_name,
                            symbol_kind: Some(sym_kind),
                            parent_scope: scope,
                            language: language.to_string(),
                            content: text.to_string(),
                            start_line: inner_child.start_position().row as u32,
                            end_line: inner_child.end_position().row as u32,
                            content_hash,
                        });
                    }
                }
                continue;
            }

            let text = &source[child.byte_range()];
            let (sym_name, sym_kind) = extract_symbol_info(&child, source);
            let scope = match (parent_name, sym_name.as_deref()) {
                (Some(p), Some(s)) => Some(format!("{}/{}", p, s)),
                (Some(p), None) => Some(p.to_string()),
                _ => None,
            };
            let content_hash = blake3::hash(text.as_bytes()).to_hex().to_string();
            sub_chunks.push(Chunk {
                file_path: file_path.to_string(),
                symbol_name: sym_name,
                symbol_kind: Some(sym_kind),
                parent_scope: scope,
                language: language.to_string(),
                content: text.to_string(),
                start_line: child.start_position().row as u32,
                end_line: child.end_position().row as u32,
                content_hash,
            });
        }

        sub_chunks
    }

    /// Fallback: split source into fixed-size line chunks.
    fn chunk_lines(&self, source: &str, language: &str, file_path: &str) -> Vec<Chunk> {
        let lines: Vec<&str> = source.lines().collect();
        if lines.is_empty() {
            return Vec::new();
        }

        // Aim for ~50 lines per chunk as a rough heuristic
        let chunk_size = 50;
        let mut chunks = Vec::new();

        for (i, window) in lines.chunks(chunk_size).enumerate() {
            let content = window.join("\n");
            let start_line = (i * chunk_size) as u32;
            let end_line = start_line + window.len() as u32 - 1;
            let content_hash = blake3::hash(content.as_bytes()).to_hex().to_string();

            chunks.push(Chunk {
                file_path: file_path.to_string(),
                symbol_name: None,
                symbol_kind: Some("block".to_string()),
                parent_scope: None,
                language: language.to_string(),
                content,
                start_line,
                end_line,
                content_hash,
            });
        }

        chunks
    }
}

// ---------------------------------------------------------------------------
// Tree-sitter language loading
// ---------------------------------------------------------------------------

fn get_tree_sitter_language(language: &str) -> Option<tree_sitter::Language> {
    match language {
        "rust" => Some(tree_sitter_rust::LANGUAGE.into()),
        "python" => Some(tree_sitter_python::LANGUAGE.into()),
        "typescript" | "javascript" => Some(tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()),
        _ => None,
    }
}

/// Chunkable AST node kinds for Rust.
fn get_rust_queries() -> &'static [&'static str] {
    &[
        "function_item",
        "struct_item",
        "impl_item",
        "trait_item",
        "enum_item",
        "mod_item",
        "macro_definition",
        "const_item",
        "static_item",
        "type_item",
    ]
}

/// Chunkable AST node kinds for Python.
fn get_python_queries() -> &'static [&'static str] {
    &[
        "function_definition",
        "class_definition",
        "decorated_definition",
    ]
}

/// Chunkable AST node kinds for TypeScript/JavaScript.
fn get_ts_queries() -> &'static [&'static str] {
    &[
        "function_declaration",
        "class_declaration",
        "interface_declaration",
        "type_alias_declaration",
        "enum_declaration",
        "lexical_declaration",
    ]
}

/// Extract symbol name and kind from a tree-sitter node.
fn extract_symbol_info(node: &tree_sitter::Node, source: &str) -> (Option<String>, String) {
    let kind = node.kind().to_string();

    // Try to find a name child node
    let name = node
        .child_by_field_name("name")
        .map(|n| source[n.byte_range()].to_string());

    (name, kind)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chunker() -> Chunker {
        Chunker::new(512)
    }

    // -----------------------------------------------------------------------
    // Rust chunking
    // -----------------------------------------------------------------------

    #[test]
    fn test_chunk_rust_function() {
        let source = r#"
fn hello() {
    println!("Hello, world!");
}
"#;
        let chunks = chunker().chunk(source, "rust", "test.rs");
        assert!(!chunks.is_empty());
        let fn_chunk = chunks.iter().find(|c| c.symbol_kind.as_deref() == Some("function_item"));
        assert!(fn_chunk.is_some(), "Should find a function_item chunk");
        let fn_chunk = fn_chunk.unwrap();
        assert_eq!(fn_chunk.symbol_name.as_deref(), Some("hello"));
        assert_eq!(fn_chunk.language, "rust");
        assert!(fn_chunk.content.contains("println!"));
    }

    #[test]
    fn test_chunk_rust_struct_and_impl() {
        let source = r#"
struct Point {
    x: f64,
    y: f64,
}

impl Point {
    fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    fn distance(&self) -> f64 {
        (self.x * self.x + self.y * self.y).sqrt()
    }
}
"#;
        let chunks = chunker().chunk(source, "rust", "point.rs");
        assert!(chunks.len() >= 2, "Should have struct + impl chunks, got {}", chunks.len());

        let struct_chunk = chunks.iter().find(|c| c.symbol_kind.as_deref() == Some("struct_item"));
        assert!(struct_chunk.is_some(), "Should find struct_item");
        assert_eq!(struct_chunk.unwrap().symbol_name.as_deref(), Some("Point"));
    }

    #[test]
    fn test_chunk_rust_preamble() {
        let source = r#"
use std::io;
use std::path::Path;

fn main() {}
"#;
        let chunks = chunker().chunk(source, "rust", "main.rs");
        let preamble = chunks.iter().find(|c| c.symbol_kind.as_deref() == Some("preamble"));
        assert!(preamble.is_some(), "Should capture preamble (use statements)");
        let preamble = preamble.unwrap();
        assert!(preamble.content.contains("use std::io"));
    }

    // -----------------------------------------------------------------------
    // Python chunking
    // -----------------------------------------------------------------------

    #[test]
    fn test_chunk_python_function() {
        let source = r#"
def greet(name):
    print(f"Hello, {name}!")
"#;
        let chunks = chunker().chunk(source, "python", "greet.py");
        assert!(!chunks.is_empty());
        let fn_chunk = chunks.iter().find(|c| c.symbol_kind.as_deref() == Some("function_definition"));
        assert!(fn_chunk.is_some(), "Should find function_definition");
        assert_eq!(fn_chunk.unwrap().symbol_name.as_deref(), Some("greet"));
    }

    #[test]
    fn test_chunk_python_class() {
        let source = r#"
class Dog:
    def __init__(self, name):
        self.name = name

    def bark(self):
        return "Woof!"
"#;
        let chunks = chunker().chunk(source, "python", "dog.py");
        assert!(!chunks.is_empty());
        let class_chunk = chunks.iter().find(|c| c.symbol_kind.as_deref() == Some("class_definition"));
        assert!(class_chunk.is_some(), "Should find class_definition");
        assert_eq!(class_chunk.unwrap().symbol_name.as_deref(), Some("Dog"));
    }

    // -----------------------------------------------------------------------
    // TypeScript chunking
    // -----------------------------------------------------------------------

    #[test]
    fn test_chunk_typescript_function() {
        let source = r#"
function add(a: number, b: number): number {
    return a + b;
}
"#;
        let chunks = chunker().chunk(source, "typescript", "math.ts");
        assert!(!chunks.is_empty());
        let fn_chunk = chunks.iter().find(|c| c.symbol_kind.as_deref() == Some("function_declaration"));
        assert!(fn_chunk.is_some(), "Should find function_declaration");
        assert_eq!(fn_chunk.unwrap().symbol_name.as_deref(), Some("add"));
    }

    // -----------------------------------------------------------------------
    // Fallback (unknown language)
    // -----------------------------------------------------------------------

    #[test]
    fn test_chunk_unknown_language_fallback() {
        let source = "line1\nline2\nline3\nline4\nline5\n";
        let chunks = chunker().chunk(source, "cobol", "legacy.cob");
        assert!(!chunks.is_empty());
        assert_eq!(chunks[0].symbol_kind.as_deref(), Some("block"));
        assert_eq!(chunks[0].language, "cobol");
    }

    #[test]
    fn test_chunk_empty_source() {
        let chunks = chunker().chunk("", "rust", "empty.rs");
        assert!(chunks.is_empty());
    }

    #[test]
    fn test_chunk_large_file_line_fallback() {
        // Generate >50 lines to test multi-chunk line splitting
        let lines: Vec<String> = (0..120).map(|i| format!("// line {}", i)).collect();
        let source = lines.join("\n");
        let chunks = chunker().chunk(&source, "text", "big.txt");
        assert!(chunks.len() >= 2, "Should split into multiple chunks, got {}", chunks.len());
    }

    // -----------------------------------------------------------------------
    // Content hashing
    // -----------------------------------------------------------------------

    #[test]
    fn test_chunk_content_hash() {
        let source = "fn foo() {}\n";
        let chunks = chunker().chunk(source, "rust", "foo.rs");
        assert!(!chunks.is_empty());
        for chunk in &chunks {
            assert!(!chunk.content_hash.is_empty(), "Hash should be non-empty");
            assert_eq!(chunk.content_hash.len(), 64, "Blake3 hex hash should be 64 chars");
        }
    }

    #[test]
    fn test_same_content_same_hash() {
        let source = "fn foo() {}\n";
        let chunks1 = chunker().chunk(source, "rust", "a.rs");
        let chunks2 = chunker().chunk(source, "rust", "b.rs");
        // Same content should produce same hash even with different file paths
        let fn_chunks1: Vec<_> = chunks1.iter().filter(|c| c.symbol_kind.as_deref() == Some("function_item")).collect();
        let fn_chunks2: Vec<_> = chunks2.iter().filter(|c| c.symbol_kind.as_deref() == Some("function_item")).collect();
        if !fn_chunks1.is_empty() && !fn_chunks2.is_empty() {
            assert_eq!(fn_chunks1[0].content_hash, fn_chunks2[0].content_hash);
        }
    }

    // -----------------------------------------------------------------------
    // Language detection helpers
    // -----------------------------------------------------------------------

    #[test]
    fn test_get_tree_sitter_language() {
        assert!(get_tree_sitter_language("rust").is_some());
        assert!(get_tree_sitter_language("python").is_some());
        assert!(get_tree_sitter_language("typescript").is_some());
        assert!(get_tree_sitter_language("javascript").is_some());
        assert!(get_tree_sitter_language("go").is_none());
        assert!(get_tree_sitter_language("cobol").is_none());
    }
}
