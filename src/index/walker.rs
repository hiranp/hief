//! File walker that respects .gitignore and .hiefignore.

use std::path::{Path, PathBuf};

use crate::errors::Result;

/// A discovered source file with metadata.
#[derive(Debug, Clone)]
pub struct FileEntry {
    pub abs_path: PathBuf,
    pub rel_path: String,
    pub language: Option<String>,
}

/// Walks the project directory, yielding source files to index.
pub struct FileWalker {
    root: PathBuf,
}

impl FileWalker {
    pub fn new(root: &Path) -> Self {
        Self {
            root: root.to_path_buf(),
        }
    }

    /// Walk the project tree and return all indexable source files.
    /// Respects .gitignore and .hiefignore automatically via the `ignore` crate.
    pub fn walk(&self) -> Result<Vec<FileEntry>> {
        let mut entries = Vec::new();

        let mut builder = ignore::WalkBuilder::new(&self.root);
        builder
            .hidden(true) // skip hidden files
            .git_ignore(true) // respect .gitignore
            .git_global(true)
            .git_exclude(true);

        // Add .hiefignore if it exists
        let hiefignore = self.root.join(".hiefignore");
        if hiefignore.exists() {
            builder.add_ignore(&hiefignore);
        }

        for result in builder.build() {
            let entry = match result {
                Ok(e) => e,
                Err(_) => continue,
            };

            // Skip directories
            if entry.file_type().is_some_and(|ft| ft.is_dir()) {
                continue;
            }

            let abs_path = entry.path().to_path_buf();
            let rel_path = abs_path
                .strip_prefix(&self.root)
                .unwrap_or(&abs_path)
                .to_string_lossy()
                .to_string();

            // Skip .hief directory itself
            if rel_path.starts_with(".hief") {
                continue;
            }

            let language = detect_language(&abs_path);

            // Only include files with a recognized language
            if language.is_some() {
                entries.push(FileEntry {
                    abs_path,
                    rel_path,
                    language,
                });
            }
        }

        Ok(entries)
    }
}

/// Detect programming language from file extension.
fn detect_language(path: &Path) -> Option<String> {
    let ext = path.extension()?.to_str()?;
    match ext {
        "rs" => Some("rust".to_string()),
        "py" | "pyi" => Some("python".to_string()),
        "ts" | "tsx" => Some("typescript".to_string()),
        "js" | "jsx" => Some("javascript".to_string()),
        "toml" => Some("toml".to_string()),
        "json" => Some("json".to_string()),
        "yaml" | "yml" => Some("yaml".to_string()),
        "md" | "markdown" => Some("markdown".to_string()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_detect_language_rust() {
        assert_eq!(
            detect_language(Path::new("main.rs")),
            Some("rust".to_string())
        );
    }

    #[test]
    fn test_detect_language_python() {
        assert_eq!(
            detect_language(Path::new("app.py")),
            Some("python".to_string())
        );
        assert_eq!(
            detect_language(Path::new("types.pyi")),
            Some("python".to_string())
        );
    }

    #[test]
    fn test_detect_language_typescript() {
        assert_eq!(
            detect_language(Path::new("index.ts")),
            Some("typescript".to_string())
        );
        assert_eq!(
            detect_language(Path::new("App.tsx")),
            Some("typescript".to_string())
        );
    }

    #[test]
    fn test_detect_language_javascript() {
        assert_eq!(
            detect_language(Path::new("script.js")),
            Some("javascript".to_string())
        );
        assert_eq!(
            detect_language(Path::new("Component.jsx")),
            Some("javascript".to_string())
        );
    }

    #[test]
    fn test_detect_language_config_files() {
        assert_eq!(
            detect_language(Path::new("Cargo.toml")),
            Some("toml".to_string())
        );
        assert_eq!(
            detect_language(Path::new("data.json")),
            Some("json".to_string())
        );
        assert_eq!(
            detect_language(Path::new("config.yaml")),
            Some("yaml".to_string())
        );
        assert_eq!(
            detect_language(Path::new("config.yml")),
            Some("yaml".to_string())
        );
        assert_eq!(
            detect_language(Path::new("README.md")),
            Some("markdown".to_string())
        );
    }

    #[test]
    fn test_detect_language_unknown() {
        assert_eq!(detect_language(Path::new("binary.exe")), None);
        assert_eq!(detect_language(Path::new("image.png")), None);
        assert_eq!(detect_language(Path::new("no_extension")), None);
    }

    #[test]
    fn test_walker_finds_files() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("main.rs"), "fn main() {}").unwrap();
        fs::write(dir.path().join("lib.py"), "def lib(): pass").unwrap();
        fs::write(dir.path().join("notes.txt"), "not indexed").unwrap();

        let walker = FileWalker::new(dir.path());
        let entries = walker.walk().unwrap();

        let languages: Vec<_> = entries
            .iter()
            .map(|e| e.language.as_deref().unwrap())
            .collect();
        assert!(languages.contains(&"rust"), "Should find .rs files");
        assert!(languages.contains(&"python"), "Should find .py files");
        // .txt has no language mapping, so it should be excluded
        assert!(!entries.iter().any(|e| e.rel_path.ends_with(".txt")));
    }

    #[test]
    fn test_walker_skips_hief_dir() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join(".hief")).unwrap();
        fs::write(dir.path().join(".hief/hief.db"), "database").unwrap();
        fs::write(dir.path().join("main.rs"), "fn main() {}").unwrap();

        let walker = FileWalker::new(dir.path());
        let entries = walker.walk().unwrap();

        assert!(!entries.iter().any(|e| e.rel_path.contains(".hief")));
        assert!(entries.iter().any(|e| e.rel_path == "main.rs"));
    }

    #[test]
    fn test_walker_empty_dir() {
        let dir = tempfile::tempdir().unwrap();
        let walker = FileWalker::new(dir.path());
        let entries = walker.walk().unwrap();
        assert!(entries.is_empty());
    }

    #[test]
    fn test_file_entry_rel_path() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("src")).unwrap();
        fs::write(dir.path().join("src/lib.rs"), "// lib").unwrap();

        let walker = FileWalker::new(dir.path());
        let entries = walker.walk().unwrap();

        let lib_entry = entries.iter().find(|e| e.rel_path.contains("lib.rs"));
        assert!(lib_entry.is_some());
        assert_eq!(lib_entry.unwrap().rel_path, "src/lib.rs");
    }
}
