//! `hief init` — project initialization.

use std::path::Path;

use crate::config::Config;
use crate::db::Database;
use crate::errors::Result;

/// Initialize HIEF in the current project.
pub async fn init(project_root: &Path) -> Result<()> {
    let hief_dir = Config::hief_dir(project_root);
    let golden_dir = hief_dir.join("golden");
    let db_path = Config::db_path(project_root);
    let config_path = project_root.join("hief.toml");
    let hiefignore_path = project_root.join(".hiefignore");

    // Create directories
    std::fs::create_dir_all(&golden_dir)?;
    println!("✅ Created {}", hief_dir.display());

    // Create database (runs migrations)
    let _db = Database::open(&db_path).await?;
    println!("✅ Created database at {}", db_path.display());

    // Write default config if it doesn't exist
    if !config_path.exists() {
        Config::write_default(&config_path)?;
        println!("✅ Created {}", config_path.display());
    } else {
        println!("⏭️  {} already exists", config_path.display());
    }

    // Write .hiefignore if it doesn't exist
    if !hiefignore_path.exists() {
        std::fs::write(&hiefignore_path, DEFAULT_HIEFIGNORE)?;
        println!("✅ Created {}", hiefignore_path.display());
    }

    // Write AGENTS.md if it doesn't exist
    let agents_path = project_root.join("AGENTS.md");
    if !agents_path.exists() {
        std::fs::write(&agents_path, DEFAULT_AGENTS_MD)?;
        println!("✅ Created AGENTS.md");
    }

    // Scaffold initial skills directory if config provides it (default .hief/skills)
    let config = Config::load(&config_path).unwrap_or_default();
    match crate::skills::scaffold_skills_dir(project_root, &config.skills) {
        Ok(report) => {
            for dir in &report.directories_created {
                println!("✅ Created {}", dir);
            }
            for file in &report.files_created {
                println!("✅ Created {}", file);
            }
            for item in &report.already_existed {
                println!("⏭️  {} already exists", item);
            }
        }
        Err(e) => {
            eprintln!("⚠️  failed to create skills directory: {}", e);
        }
    }

    // Create a conventions.toml placeholder if missing
    let conv_path = project_root.join(".hief").join("conventions.toml");
    if !conv_path.exists() {
        let placeholder = "# HIEF conventions toml\n# Define project rules here.\n";
        std::fs::write(&conv_path, placeholder)?;
        println!("✅ Created {}", conv_path.display());
    }

    // Append to .gitignore
    let gitignore_path = project_root.join(".gitignore");
    let gitignore_entry = ".hief/hief.db\n.hief/hief.db-*\n";
    if gitignore_path.exists() {
        let content = std::fs::read_to_string(&gitignore_path)?;
        if !content.contains(".hief/hief.db") {
            std::fs::write(
                &gitignore_path,
                format!("{}\n{}", content.trim(), gitignore_entry),
            )?;
            println!("✅ Updated .gitignore");
        }
    } else {
        std::fs::write(&gitignore_path, gitignore_entry)?;
        println!("✅ Created .gitignore");
    }

    println!("\n🎉 HIEF initialized! Run `hief index build` to index your codebase.");
    Ok(())
}

// ---------------------------------------------------------------------------
// Default file contents used by `hief init`
// ---------------------------------------------------------------------------

pub(crate) const DEFAULT_HIEFIGNORE: &str = r#"# HIEF ignore patterns (in addition to .gitignore)
# These files/directories will be skipped during indexing

# Build artifacts
target/
dist/
build/
node_modules/
__pycache__/
*.pyc

# Generated files
*.generated.*
*.min.js
*.min.css

# Large binary files
*.wasm
*.so
*.dylib
*.dll

# Lock files
Cargo.lock
package-lock.json
yarn.lock
pnpm-lock.yaml

# IDE files
.vscode/
.idea/
*.swp
*.swo
"#;

pub(crate) const DEFAULT_AGENTS_MD: &str = r#"# AGENTS.md

This project uses **HIEF** (Hybrid Intent-Evaluation Framework) for agent coordination, indexing, and
evaluation. The document follows the AAIF AGENTS.md conventions.

## Local MCP Server

Start the host agent's local MCP server before doing anything:

```sh
hief serve         # stdio transport
hief serve --transport http --port 3100   # http transport
```

## Typical Workflow

1. **Search code** — Use the `search_code` tool to find definitions and related logic before touching files.
2. **Create an intent** — Every change must begin with `create_intent`. The intent captures the task.
3. **Update intent status** — `in_progress` when starting, `in_review` when done.
4. **Run evaluation** — Use `run_evaluation` to check code quality against golden sets.
5. **Commit & push** — All intents and evaluation results live in git.

## Best Practices

* Always search the codebase before making changes.
* Create small, atomic intents for reviewability.
* Run evaluations before marking work as complete.
* No agent may mark its own intent `approved` — a human must review.
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_init_creates_core_files() {
        let root = tempdir().unwrap();
        init(root.path()).await.unwrap();
        assert!(root.path().join(".hief/golden").exists());
        assert!(root.path().join("hief.toml").exists());
        assert!(root.path().join(".hief/conventions.toml").exists());
        assert!(root.path().join(".hief/skills/README.md").exists());
        assert!(root.path().join("AGENTS.md").exists());
    }
}
