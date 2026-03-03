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
