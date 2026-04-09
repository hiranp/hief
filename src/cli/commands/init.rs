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

    // Create a conventions.toml — auto-generated from the project's manifest if possible
    let conv_path = project_root.join(".hief").join("conventions.toml");
    if !conv_path.exists() {
        let conventions = generate_conventions(project_root);
        std::fs::write(&conv_path, conventions)?;
        println!(
            "✅ Created {} (auto-generated from project — review and customise)",
            conv_path.display()
        );
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
    // Scaffold context/, patterns/, and router.toml
    scaffold_context_dir(project_root)?;
    scaffold_patterns_dir(project_root)?;
    scaffold_router(project_root)?;

    // Generate multi-tool config files
    let profile = ProjectProfile::detect(project_root);
    scaffold_multi_tool_configs(project_root, &profile)?;

    println!("\n🎉 HIEF initialized! Run `hief index build` to index your codebase.");
    Ok(())
}

// ---------------------------------------------------------------------------
// Context directory scaffolding
// ---------------------------------------------------------------------------

fn scaffold_context_dir(project_root: &Path) -> Result<()> {
    use crate::context::write_context_file;

    let context_dir = project_root.join(".hief").join("context");
    if context_dir.exists() {
        println!("⏭️  .hief/context/ already exists");
        return Ok(());
    }
    std::fs::create_dir_all(&context_dir)?;

    let stubs: &[(&str, &str)] = &[
        (
            "architecture",
            "# Architecture\n\n<!-- Describe top-level components, their responsibilities, and how they connect. Update after each design decision. -->\n",
        ),
        (
            "conventions",
            "# Conventions\n\n<!-- Project-specific coding conventions beyond what's in conventions.toml. Human-readable rationale lives here. -->\n",
        ),
        (
            "setup",
            "# Setup\n\n<!-- Local development setup steps. Keep this current so new agents can onboard quickly. -->\n",
        ),
        (
            "stack",
            "# Stack\n\n<!-- Languages, frameworks, key libraries, and runtime requirements. -->\n",
        ),
        (
            "decisions",
            "# Decisions Log\n\n<!-- Record significant architectural and design decisions with their rationale. -->\n",
        ),
    ];

    for (name, content) in stubs {
        if let Err(e) = write_context_file(project_root, name, content) {
            eprintln!("⚠️  failed to create context/{}.md: {}", name, e);
        } else {
            println!("✅ Created .hief/context/{}.md", name);
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Patterns directory scaffolding
// ---------------------------------------------------------------------------

fn scaffold_patterns_dir(project_root: &Path) -> Result<()> {
    let patterns_dir = project_root.join(".hief").join("patterns");
    if patterns_dir.exists() {
        println!("⏭️  .hief/patterns/ already exists");
        return Ok(());
    }
    std::fs::create_dir_all(&patterns_dir)?;
    if let Err(e) = crate::patterns::sync_index(project_root) {
        eprintln!("⚠️  failed to initialize patterns INDEX.md: {}", e);
    } else {
        println!("✅ Created .hief/patterns/INDEX.md");
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Router scaffolding
// ---------------------------------------------------------------------------

fn scaffold_router(project_root: &Path) -> Result<()> {
    let router_path = project_root.join(".hief").join("router.toml");
    if router_path.exists() {
        println!("⏭️  .hief/router.toml already exists");
        return Ok(());
    }
    if let Err(e) = crate::router::write_default(project_root) {
        eprintln!("⚠️  failed to create .hief/router.toml: {}", e);
    } else {
        println!("✅ Created .hief/router.toml (default routing table)");
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Multi-tool config scaffolding (CLAUDE.md, .cursorrules, copilot-instructions.md)
// ---------------------------------------------------------------------------

fn scaffold_multi_tool_configs(project_root: &Path, profile: &ProjectProfile) -> Result<()> {
    let hief_block = build_multi_tool_block(profile);

    // CLAUDE.md
    let claude_path = project_root.join("CLAUDE.md");
    if !claude_path.exists() {
        std::fs::write(&claude_path, format!("{}\n", hief_block))?;
        println!("✅ Created CLAUDE.md (HIEF session protocol)");
    }

    // .cursorrules
    let cursor_path = project_root.join(".cursorrules");
    if !cursor_path.exists() {
        std::fs::write(&cursor_path, format!("{}\n", hief_block))?;
        println!("✅ Created .cursorrules");
    }

    // .windsurfrules
    let windsurf_path = project_root.join(".windsurfrules");
    if !windsurf_path.exists() {
        std::fs::write(&windsurf_path, format!("{}\n", hief_block))?;
        println!("✅ Created .windsurfrules");
    }

    // .github/copilot-instructions.md
    let copilot_dir = project_root.join(".github");
    let copilot_path = copilot_dir.join("copilot-instructions.md");
    if !copilot_path.exists() {
        std::fs::create_dir_all(&copilot_dir)?;
        std::fs::write(&copilot_path, format!("{}\n", hief_block))?;
        println!("✅ Created .github/copilot-instructions.md");
    }

    Ok(())
}

fn build_multi_tool_block(profile: &ProjectProfile) -> String {
    let mut out = String::from(
        "# HIEF Agent Instructions\n\n\
         This project uses **HIEF** for persistent memory and agent coordination.\n\n\
         ## Session Protocol\n\n\
         Every session MUST begin with:\n\
         1. `get_project_context` — index stats, active intents\n\
         2. `get_conventions` — project coding rules\n\
         3. `get_routing_table` — choose relevant context/patterns for your task type\n\
         4. `get_session_context` — resume from prior session\n\n\
         ## GROW Step (after every task)\n\n\
         After completing work:\n\
         1. `write_context_file` — update architecture.md or decisions.md if design changed\n\
         2. `check_drift` — verify scaffold is in sync (score should be ≥ 90)\n\n\
         ## Code Search\n\n\
         - `search_code \"term\"` — keyword search (FTS5)\n\
         - `structural_search \"$X.unwrap()\" rust` — AST pattern search\n\
         - `find_callers \"function_name\" rust` — find all call sites\n\n\
         ## Intents\n\n\
         Every non-trivial change needs an intent:\n\
         ```\n\
         create_intent(kind=\"feature\", title=\"Add ...\")\n\
         update_intent(id=..., status=\"in_progress\")\n\
         ```\n\n",
    );

    if profile.has_rust {
        out.push_str(
            "## Rust Conventions\n\n\
             - No `.unwrap()` in `src/` — use `?` or `.expect(\"context\")`\n\
             - All public functions must have `///` doc comments\n\
             - Error types implement `thiserror::Error`\n\
             - Async runtime: `tokio` only\n\n",
        );
    }

    if profile.has_typescript {
        out.push_str(
            "## TypeScript Conventions\n\n\
             - No `any` type — use `unknown` or proper generics\n\
             - No `console.log` in production — use structured logger\n\n",
        );
    }

    if profile.has_python {
        out.push_str(
            "## Python Conventions\n\n\
             - No bare `except:` — catch specific exception types\n\
             - Type hints required on all public functions\n\n",
        );
    }

    out.push_str(
        "## Patterns\n\n\
         Use `list_patterns` to discover project-specific guides stored in `.hief/patterns/`.\n\
         Create new patterns with `create_pattern` after solving a recurring problem.\n",
    );

    out
}

// ---------------------------------------------------------------------------
// Convention auto-generation
// ---------------------------------------------------------------------------

/// Detected language stack for generating conventions.
#[derive(Default)]
struct ProjectProfile {
    has_rust: bool,
    has_typescript: bool,
    has_python: bool,
    uses_tokio: bool,
    uses_thiserror: bool,
    uses_sqlx: bool,
    uses_react: bool,
    uses_pytest: bool,
}

impl ProjectProfile {
    fn detect(root: &std::path::Path) -> Self {
        let mut p = Self::default();

        // Rust: Cargo.toml
        let cargo_path = root.join("Cargo.toml");
        if cargo_path.exists() {
            p.has_rust = true;
            if let Ok(content) = std::fs::read_to_string(&cargo_path) {
                p.uses_tokio = content.contains("tokio");
                p.uses_thiserror = content.contains("thiserror");
                p.uses_sqlx = content.contains("sqlx") || content.contains("libsql");
            }
        }

        // TypeScript / JavaScript: package.json
        let pkg_path = root.join("package.json");
        if pkg_path.exists() {
            p.has_typescript = true; // treat JS projects the same rule set
            if let Ok(content) = std::fs::read_to_string(&pkg_path) {
                p.uses_react = content.contains("\"react\"") || content.contains("\"next\"");
            }
        }

        // Python: pyproject.toml or setup.py or requirements.txt
        if root.join("pyproject.toml").exists()
            || root.join("setup.py").exists()
            || root.join("requirements.txt").exists()
        {
            p.has_python = true;
            if let Ok(content) = std::fs::read_to_string(root.join("pyproject.toml")) {
                p.uses_pytest = content.contains("pytest");
            }
            if root.join("pytest.ini").exists() || root.join("conftest.py").exists() {
                p.uses_pytest = true;
            }
        }

        p
    }
}

/// Scan the project root and generate a conventions.toml tailored to the
/// detected language stack and dependencies.
///
/// This removes the cold-start burden: developers get a working set of rules
/// immediately after `hief init` instead of a blank file they must write by hand.
/// The output is intentionally marked "auto-generated" so teams know to review it.
fn generate_conventions(root: &std::path::Path) -> String {
    let profile = ProjectProfile::detect(root);

    let mut out = String::from(
        "# HIEF Project Conventions (auto-generated by `hief init` — review and customise)\n\
         #\n\
         # Machine-readable rules that agents MUST follow when writing code.\n\
         # Each rule can carry severity = \"error\" | \"warning\" | \"info\".\n\
         #\n\
         # Structural check_pattern values use ast-grep syntax.\n\
         # Literal patterns are matched as substrings across indexed chunks.\n\
         #\n\
         # Re-run `hief index build` after editing this file so changes are reflected.\n\n",
    );

    if profile.has_rust {
        out.push_str(
            "# ---------------------------------------------------------------------------\n\
             # Rust — Error Handling\n\
             # ---------------------------------------------------------------------------\n\n\
             [error_handling.no_unwrap]\n\
             description = \"No bare .unwrap() in production code\"\n\
             check_pattern = \"$X.unwrap()\"\n\
             language = \"rust\"\n\
             scope = \"src/**/*.rs\"\n\
             exclude = [\"tests/**\", \"src/**/tests.rs\"]\n\
             severity = \"error\"\n\
             rationale = \"unwrap() panics on None/Err. Propagate with ? or use .expect() with context.\"\n\n",
        );

        if profile.uses_thiserror {
            out.push_str(
                "[error_handling.thiserror_derive]\n\
                 description = \"Error enums must derive thiserror::Error\"\n\
                 check_pattern = \"#[derive(Error)]\"\n\
                 language = \"rust\"\n\
                 scope = \"src/**/*.rs\"\n\
                 severity = \"warning\"\n\
                 rationale = \"thiserror keeps error Display consistent and implements std::error::Error automatically.\"\n\n",
            );
        }

        out.push_str(
            "# ---------------------------------------------------------------------------\n\
             # Rust — Documentation\n\
             # ---------------------------------------------------------------------------\n\n\
             [documentation.pub_fn_doc_comments]\n\
             description = \"All public functions must have /// doc comments\"\n\
             check_pattern = \"/// $DOC\\npub fn $NAME\"\n\
             language = \"rust\"\n\
             scope = \"src/**/*.rs\"\n\
             severity = \"warning\"\n\
             rationale = \"Doc comments are indexed by HIEF for code search and power IDE tooltips.\"\n\n\
             [documentation.module_docs]\n\
             description = \"Every module file must start with //! module-level docs\"\n\
             check_pattern = \"//! $DOC\"\n\
             language = \"rust\"\n\
             scope = \"src/**/*.rs\"\n\
             severity = \"info\"\n\
             rationale = \"Module docs explain purpose and appear in search results.\"\n\n",
        );

        out.push_str(
            "# ---------------------------------------------------------------------------\n\
             # Rust — Architecture\n\
             # ---------------------------------------------------------------------------\n\n\
             [architecture.no_unsafe]\n\
             description = \"No unsafe blocks without documented safety invariants\"\n\
             check_pattern = \"unsafe { $$$BODY }\"\n\
             language = \"rust\"\n\
             scope = \"src/**/*.rs\"\n\
             severity = \"error\"\n\
             rationale = \"unsafe requires explicit // SAFETY: comment above the block.\"\n\n",
        );

        if profile.uses_tokio {
            out.push_str(
                "[architecture.tokio_runtime_only]\n\
                 description = \"Async runtime: tokio only (no async-std)\"\n\
                 scope = \"src/**/*.rs, Cargo.toml\"\n\
                 literal_must_not_contain = \"async-std\"\n\
                 severity = \"error\"\n\
                 rationale = \"Mixing runtimes causes panics. Project uses tokio exclusively.\"\n\n",
            );
        }

        if profile.uses_sqlx {
            out.push_str(
                "[architecture.db_via_abstraction]\n\
                 description = \"All database access goes through the project Database wrapper\"\n\
                 scope = \"src/**/*.rs\"\n\
                 severity = \"warning\"\n\
                 rationale = \"Direct DB handles bypass connection pooling and error handling conventions.\"\n\n",
            );
        }
    }

    if profile.has_typescript {
        out.push_str(
            "# ---------------------------------------------------------------------------\n\
             # TypeScript — Code Quality\n\
             # ---------------------------------------------------------------------------\n\n\
             [ts.no_any]\n\
             description = \"Avoid the 'any' type — use 'unknown' or proper generics\"\n\
             scope = \"**/*.ts, **/*.tsx\"\n\
             literal_must_not_contain = \": any\"\n\
             severity = \"warning\"\n\
             rationale = \"'any' defeats TypeScript's type safety and breaks refactoring tools.\"\n\n\
             [ts.no_console_log]\n\
             description = \"No console.log in production code — use a structured logger\"\n\
             scope = \"src/**/*.ts, src/**/*.tsx\"\n\
             literal_must_not_contain = \"console.log\"\n\
             severity = \"warning\"\n\
             rationale = \"console.log leaks internals and cannot be filtered in production.\"\n\n",
        );

        if profile.uses_react {
            out.push_str(
                "[ts.react_key_prop]\n\
                 description = \"List items rendered with .map() must include a stable key prop\"\n\
                 scope = \"src/**/*.tsx, src/**/*.jsx\"\n\
                 severity = \"warning\"\n\
                 rationale = \"Missing keys cause React reconciliation bugs and console warnings.\"\n\n",
            );
        }
    }

    if profile.has_python {
        out.push_str(
            "# ---------------------------------------------------------------------------\n\
             # Python — Code Quality\n\
             # ---------------------------------------------------------------------------\n\n\
             [python.no_bare_except]\n\
             description = \"No bare 'except:' clauses — catch specific exception types\"\n\
             scope = \"**/*.py\"\n\
             literal_must_not_contain = \"except:\"\n\
             severity = \"error\"\n\
             rationale = \"Bare except swallows KeyboardInterrupt and SystemExit, masking bugs.\"\n\n\
             [python.type_hints]\n\
             description = \"Public functions should have type annotations\"\n\
             scope = \"**/*.py\"\n\
             severity = \"info\"\n\
             rationale = \"Type hints enable static analysis (mypy/pyright) and IDE intelligence.\"\n\n",
        );

        if profile.uses_pytest {
            out.push_str(
                "[python.pytest_prefix]\n\
                 description = \"Test functions must be prefixed with test_\"\n\
                 scope = \"tests/**/*.py, test_*.py\"\n\
                 severity = \"info\"\n\
                 rationale = \"pytest collects only functions prefixed test_; other names are silently skipped.\"\n\n",
            );
        }
    }

    // Fall-back if nothing was detected
    if !profile.has_rust && !profile.has_typescript && !profile.has_python {
        out.push_str(
            "# No language manifest detected. Add rules here after running `hief index build`.\n\
             # Example:\n\
             #\n\
             # [error_handling.no_debug_print]\n\
             # description = \"No debug print statements in production code\"\n\
             # scope = \"src/**/*\"\n\
             # literal_must_not_contain = \"TODO\"\n\
             # severity = \"warning\"\n",
        );
    }

    out
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
        let root = tempdir().expect("failed to create tempdir");
        init(root.path()).await.expect("init failed");
        assert!(root.path().join(".hief/golden").exists());
        assert!(root.path().join("hief.toml").exists());
        assert!(root.path().join(".hief/conventions.toml").exists());
        assert!(root.path().join(".hief/skills/README.md").exists());
        assert!(root.path().join("AGENTS.md").exists());
    }
}
