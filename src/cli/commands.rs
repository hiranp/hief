//! CLI command implementations.

use std::path::Path;

use serde::Serialize;

use crate::config::Config;
use crate::db::Database;
use crate::errors::Result;
use crate::graph;
use crate::graph::edges::IntentEdge;
use crate::graph::intent::Intent;

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
        std::fs::write(
            &hiefignore_path,
            DEFAULT_HIEFIGNORE,
        )?;
        println!("✅ Created {}", hiefignore_path.display());
    }

    // Write AGENTS.md if it doesn't exist
    let agents_path = project_root.join("AGENTS.md");
    if !agents_path.exists() {
        std::fs::write(&agents_path, DEFAULT_AGENTS_MD)?;
        println!("✅ Created AGENTS.md");
    }

    // Append to .gitignore
    let gitignore_path = project_root.join(".gitignore");
    let gitignore_entry = ".hief/hief.db\n.hief/hief.db-*\n";
    if gitignore_path.exists() {
        let content = std::fs::read_to_string(&gitignore_path)?;
        if !content.contains(".hief/hief.db") {
            std::fs::write(&gitignore_path, format!("{}\n{}", content.trim(), gitignore_entry))?;
            println!("✅ Updated .gitignore");
        }
    } else {
        std::fs::write(&gitignore_path, gitignore_entry)?;
        println!("✅ Created .gitignore");
    }

    println!("\n🎉 HIEF initialized! Run `hief index build` to index your codebase.");
    Ok(())
}

/// Build or update the code index.
pub async fn index_build(db: &Database, project_root: &Path, config: &Config, json: bool) -> Result<()> {
    let report = crate::index::build(db, project_root, &config.index).await?;

    if json {
        println!("{}", serde_json::to_string_pretty(&report).unwrap());
    } else {
        println!(
            "📦 Indexed {} files ({} new, {} updated, {} removed)",
            report.files_new + report.files_updated,
            report.files_new,
            report.files_updated,
            report.files_removed,
        );
        println!("   {} total chunks, {}ms", report.total_chunks, report.duration_ms);
    }

    Ok(())
}

/// Search the code index.
pub async fn index_search(
    db: &Database,
    query: &str,
    top_k: usize,
    language: Option<&str>,
    kind: Option<&str>,
    json: bool,
) -> Result<()> {
    let mut search_query = crate::index::search::SearchQuery::new(query);
    search_query.top_k = top_k;
    if let Some(lang) = language {
        search_query.language = Some(lang.to_string());
    }
    if let Some(k) = kind {
        search_query.symbol_kind = Some(k.to_string());
    }

    let results = crate::index::search(db, &search_query).await?;

    if json {
        println!("{}", serde_json::to_string_pretty(&results).unwrap());
    } else if results.is_empty() {
        println!("No results found for '{}'", query);
    } else {
        println!("Found {} results for '{}':\n", results.len(), query);
        for (i, r) in results.iter().enumerate() {
            let symbol = r.symbol_name.as_deref().unwrap_or("(anonymous)");
            let kind = r.symbol_kind.as_deref().unwrap_or("");
            println!(
                "  {}. {} [{}] — {}:{}–{}",
                i + 1,
                symbol,
                kind,
                r.file_path,
                r.start_line,
                r.end_line,
            );
            // Show snippet (first 3 lines)
            for line in r.content.lines().take(3) {
                println!("     {}", line);
            }
            println!();
        }
    }

    Ok(())
}

/// Show index statistics.
pub async fn index_status(db: &Database, project_root: &Path, json: bool) -> Result<()> {
    let stats = crate::index::status(db, project_root).await?;

    if json {
        println!("{}", serde_json::to_string_pretty(&stats).unwrap());
    } else {
        println!("📊 Index Status:");
        println!("   Files: {}", stats.total_files);
        println!("   Chunks: {}", stats.total_chunks);
        println!("   DB size: {} bytes", stats.db_size_bytes);
        if let Some(ts) = stats.last_indexed {
            println!("   Last indexed: {}", ts);
        }
        println!("   Languages:");
        for (lang, count) in &stats.languages {
            println!("     {}: {} files", lang, count);
        }
    }

    Ok(())
}

/// Create a new intent.
pub async fn graph_create(
    db: &Database,
    kind: &str,
    title: &str,
    description: Option<&str>,
    priority: &str,
    depends_on: Option<&str>,
    json: bool,
) -> Result<()> {
    let intent = Intent::new(kind, title, description.map(String::from), Some(priority.to_string()));

    graph::create_intent(db, &intent).await?;

    // Add dependency edges if specified
    if let Some(deps) = depends_on {
        for dep_id in deps.split(',').map(|s| s.trim()) {
            if !dep_id.is_empty() {
                let edge = IntentEdge::depends_on(&intent.id, dep_id);
                graph::add_edge(db, &edge).await?;
            }
        }
    }

    if json {
        println!("{}", serde_json::to_string_pretty(&intent).unwrap());
    } else {
        println!("✅ Created intent: {} ({})", intent.id, intent.title);
    }

    Ok(())
}

/// List intents.
pub async fn graph_list(
    db: &Database,
    status: Option<&str>,
    kind: Option<&str>,
    json: bool,
) -> Result<()> {
    let intents = graph::list_intents(db, status, kind).await?;

    if json {
        println!("{}", serde_json::to_string_pretty(&intents).unwrap());
    } else if intents.is_empty() {
        println!("No intents found.");
    } else {
        println!("📋 Intents ({}):\n", intents.len());
        for i in &intents {
            let assigned = i.assigned_to.as_deref().unwrap_or("unassigned");
            println!(
                "  {} [{}] {} — {} ({})",
                status_icon(&i.status),
                i.kind,
                i.title,
                i.status,
                assigned,
            );
            println!("    ID: {}", i.id);
        }
    }

    Ok(())
}

/// Show intent details (supports short ID prefix resolution).
pub async fn graph_show(db: &Database, id: &str, json: bool) -> Result<()> {
    let resolved_id = graph::resolve_id(db, id).await?;
    let intent_with_deps = graph::get_intent_with_deps(db, &resolved_id).await?;

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&intent_with_deps).unwrap()
        );
    } else {
        let i = &intent_with_deps.intent;
        println!("📌 Intent: {}", i.title);
        println!("   ID: {}", i.id);
        println!("   Kind: {}", i.kind);
        println!("   Status: {} {}", status_icon(&i.status), i.status);
        println!("   Priority: {}", i.priority);
        if let Some(desc) = &i.description {
            println!("   Description: {}", desc);
        }
        if let Some(assigned) = &i.assigned_to {
            println!("   Assigned to: {}", assigned);
        }
        if !i.criteria.is_empty() {
            println!("   Criteria:");
            for c in &i.criteria {
                println!("     - {}", c);
            }
        }
        println!(
            "   Dependencies satisfied: {}",
            if intent_with_deps.all_deps_satisfied {
                "✅ yes"
            } else {
                "❌ no"
            }
        );
        if !intent_with_deps.depends_on.is_empty() {
            println!("   Depends on:");
            for dep in &intent_with_deps.depends_on {
                println!(
                    "     {} {} — {} ({})",
                    status_icon(&dep.status),
                    dep.title,
                    dep.status,
                    dep.id,
                );
            }
        }
        if !intent_with_deps.blocks.is_empty() {
            println!("   Blocks:");
            for blk in &intent_with_deps.blocks {
                println!("     {} ({})", blk.title, blk.id);
            }
        }
    }

    Ok(())
}

/// Update an intent (supports short ID prefix resolution).
pub async fn graph_update(
    db: &Database,
    id: &str,
    status: Option<&str>,
    assign: Option<&str>,
    json: bool,
) -> Result<()> {
    let resolved_id = graph::resolve_id(db, id).await?;
    let id = resolved_id.as_str();

    if let Some(new_status) = status {
        graph::update_status(db, id, new_status).await?;
        if !json {
            println!("✅ Updated status to '{}'", new_status);
        }
    }

    if let Some(assignee) = assign {
        graph::assign_intent(db, id, assignee).await?;
        if !json {
            println!("✅ Assigned to '{}'", assignee);
        }
    }

    if json {
        let intent = graph::get_intent(db, id).await?;
        println!("{}", serde_json::to_string_pretty(&intent).unwrap());
    }

    Ok(())
}

/// Show ready intents.
pub async fn graph_ready(db: &Database, json: bool) -> Result<()> {
    let intents = graph::ready_intents(db).await?;

    if json {
        println!("{}", serde_json::to_string_pretty(&intents).unwrap());
    } else if intents.is_empty() {
        println!("No intents ready for work.");
    } else {
        println!("🚀 Ready intents ({}):\n", intents.len());
        for i in &intents {
            println!("  [{}] {} — {}", i.kind, i.title, i.priority);
            println!("    ID: {}", i.id);
        }
    }

    Ok(())
}

/// Validate graph integrity.
pub async fn graph_validate(db: &Database, json: bool) -> Result<()> {
    let validation = graph::validate_graph(db).await?;

    if json {
        println!("{}", serde_json::to_string_pretty(&validation).unwrap());
    } else {
        if validation.has_cycles {
            println!("⚠️  Cycles detected in {} nodes:", validation.cycle_nodes.len());
            for node in &validation.cycle_nodes {
                println!("    - {}", node);
            }
        } else {
            println!("✅ No cycles detected");
        }
        if validation.auto_blocked > 0 {
            println!(
                "🔒 {} intents auto-blocked (depend on rejected intents)",
                validation.auto_blocked
            );
        }
    }

    Ok(())
}

/// Run evaluation.
pub async fn eval_run(
    db: &Database,
    project_root: &Path,
    config: &Config,
    golden: Option<&str>,
    ci: bool,
    json: bool,
) -> Result<i32> {
    if ci {
        let exit_code =
            crate::eval::run_ci(db, project_root, &config.eval, golden).await?;
        return Ok(exit_code);
    }

    let results = crate::eval::run(db, project_root, &config.eval, golden).await?;

    if json {
        println!("{}", serde_json::to_string_pretty(&results).unwrap());
    } else {
        for result in &results {
            let icon = if result.passed { "✅" } else { "❌" };
            println!(
                "{} Golden set '{}': score {:.2} ({})",
                icon,
                result.golden_set,
                result.overall_score,
                if result.passed { "PASS" } else { "FAIL" },
            );
            for case in &result.cases {
                let case_icon = if case.passed { "✓" } else { "✗" };
                println!(
                    "  {} [{}] {} — {:.2}",
                    case_icon, case.priority, case.case_name, case.score,
                );
                for v in &case.violations {
                    println!("    ⚠ {}: '{}' in {}", v.kind, v.pattern, v.file);
                }
            }
        }
    }

    Ok(0)
}

/// Show evaluation report.
pub async fn eval_report(
    db: &Database,
    golden: &str,
    limit: usize,
    json: bool,
) -> Result<()> {
    let history =
        crate::eval::history::get_history(db, golden, limit).await?;

    if json {
        println!("{}", serde_json::to_string_pretty(&history).unwrap());
    } else if history.is_empty() {
        println!("No evaluation history for '{}'", golden);
    } else {
        println!("📈 Score history for '{}' (last {}):\n", golden, limit);
        for entry in &history {
            let icon = if entry.passed { "✅" } else { "❌" };
            let commit = entry
                .git_commit
                .as_deref()
                .map(|c| &c[..7.min(c.len())])
                .unwrap_or("N/A");
            println!(
                "  {} {:.2} — commit {} (ts: {})",
                icon, entry.score, commit, entry.created_at,
            );
        }
    }

    Ok(())
}

/// List golden sets.
pub fn eval_golden_list(project_root: &Path, config: &Config, json: bool) -> Result<()> {
    let sets = crate::eval::list_golden_sets(project_root, &config.eval)?;

    if json {
        println!("{}", serde_json::to_string_pretty(&sets).unwrap());
    } else if sets.is_empty() {
        println!("No golden sets found in {}", config.eval.golden_set_path);
    } else {
        println!("📝 Golden sets:");
        for name in &sets {
            println!("  - {}", name);
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Doctor command
// ---------------------------------------------------------------------------

/// Result of a doctor health check.
#[derive(Debug, Clone, Serialize)]
pub struct DoctorReport {
    pub healthy: bool,
    pub checks: Vec<DoctorCheck>,
    pub fixes_applied: usize,
}

/// A single health check result.
#[derive(Debug, Clone, Serialize)]
pub struct DoctorCheck {
    pub name: String,
    pub status: String, // "ok", "warning", "error"
    pub message: String,
    pub fixable: bool,
    pub fixed: bool,
}

/// Run health checks on index, graph, and eval subsystems.
pub async fn doctor(
    db: &Database,
    project_root: &Path,
    config: &Config,
    fix: bool,
    json: bool,
) -> Result<()> {
    let mut checks = Vec::new();
    let mut fixes_applied = 0;

    // 1. Check if .hief directory exists
    let hief_dir = Config::hief_dir(project_root);
    if !hief_dir.exists() {
        checks.push(DoctorCheck {
            name: "hief_init".to_string(),
            status: "error".to_string(),
            message: "HIEF not initialized — run `hief init`".to_string(),
            fixable: false,
            fixed: false,
        });
    } else {
        checks.push(DoctorCheck {
            name: "hief_init".to_string(),
            status: "ok".to_string(),
            message: ".hief directory exists".to_string(),
            fixable: false,
            fixed: false,
        });
    }

    // 2. Check index staleness
    let stats = crate::index::status(db, project_root).await?;
    if stats.total_files == 0 {
        let mut check = DoctorCheck {
            name: "index_populated".to_string(),
            status: "warning".to_string(),
            message: "Index is empty — run `hief index build`".to_string(),
            fixable: true,
            fixed: false,
        };
        if fix {
            crate::index::build(db, project_root, &config.index).await?;
            check.status = "ok".to_string();
            check.message = "Index rebuilt successfully".to_string();
            check.fixed = true;
            fixes_applied += 1;
        }
        checks.push(check);
    } else {
        checks.push(DoctorCheck {
            name: "index_populated".to_string(),
            status: "ok".to_string(),
            message: format!("{} files, {} chunks indexed", stats.total_files, stats.total_chunks),
            fixable: false,
            fixed: false,
        });
    }

    // 3. Check graph integrity (cycles)
    let validation = graph::validate_graph(db).await?;
    if validation.has_cycles {
        checks.push(DoctorCheck {
            name: "graph_cycles".to_string(),
            status: "error".to_string(),
            message: format!(
                "Dependency cycles detected in {} nodes: {}",
                validation.cycle_nodes.len(),
                validation.cycle_nodes.join(", ")
            ),
            fixable: false,
            fixed: false,
        });
    } else {
        checks.push(DoctorCheck {
            name: "graph_cycles".to_string(),
            status: "ok".to_string(),
            message: "No dependency cycles".to_string(),
            fixable: false,
            fixed: false,
        });
    }

    // 4. Check for auto-blocked intents (blocked status from rejected deps)
    if validation.auto_blocked > 0 {
        checks.push(DoctorCheck {
            name: "graph_auto_blocked".to_string(),
            status: "warning".to_string(),
            message: format!(
                "{} intents auto-blocked due to rejected dependencies",
                validation.auto_blocked
            ),
            fixable: false,
            fixed: false,
        });
    }

    // 5. Check for stale intents (in_progress for too long)
    let all_intents = graph::list_intents(db, None, None).await?;
    let stale_intents: Vec<_> = all_intents
        .iter()
        .filter(|i| {
            i.status == "in_progress" && {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs() as i64;
                // Stale if in_progress for more than 7 days
                now - i.updated_at > 7 * 24 * 3600
            }
        })
        .collect();

    if !stale_intents.is_empty() {
        checks.push(DoctorCheck {
            name: "stale_intents".to_string(),
            status: "warning".to_string(),
            message: format!(
                "{} intents stuck in 'in_progress' for >7 days: {}",
                stale_intents.len(),
                stale_intents
                    .iter()
                    .map(|i| i.id.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            fixable: false,
            fixed: false,
        });
    } else {
        checks.push(DoctorCheck {
            name: "stale_intents".to_string(),
            status: "ok".to_string(),
            message: "No stale intents".to_string(),
            fixable: false,
            fixed: false,
        });
    }

    // 6. Check for orphaned intents (no edges and still in draft)
    let draft_intents: Vec<_> = all_intents
        .iter()
        .filter(|i| i.status == "draft")
        .collect();
    if draft_intents.len() > 10 {
        checks.push(DoctorCheck {
            name: "orphan_drafts".to_string(),
            status: "warning".to_string(),
            message: format!(
                "{} intents still in draft — consider reviewing or removing",
                draft_intents.len()
            ),
            fixable: false,
            fixed: false,
        });
    } else {
        checks.push(DoctorCheck {
            name: "orphan_drafts".to_string(),
            status: "ok".to_string(),
            message: format!("{} draft intents", draft_intents.len()),
            fixable: false,
            fixed: false,
        });
    }

    // 7. Check golden sets exist
    let golden_sets = crate::eval::list_golden_sets(project_root, &config.eval)?;
    if golden_sets.is_empty() {
        checks.push(DoctorCheck {
            name: "golden_sets".to_string(),
            status: "warning".to_string(),
            message: format!(
                "No golden sets found in {} — evaluation won't work",
                config.eval.golden_set_path
            ),
            fixable: false,
            fixed: false,
        });
    } else {
        checks.push(DoctorCheck {
            name: "golden_sets".to_string(),
            status: "ok".to_string(),
            message: format!("{} golden sets available", golden_sets.len()),
            fixable: false,
            fixed: false,
        });
    }

    // 8. Check git hooks
    let hooks_dir = project_root.join(".git/hooks");
    let post_commit_hook = hooks_dir.join("post-commit");
    if !post_commit_hook.exists()
        || !std::fs::read_to_string(&post_commit_hook)
            .unwrap_or_default()
            .contains("hief")
    {
        let mut check = DoctorCheck {
            name: "git_hooks".to_string(),
            status: "warning".to_string(),
            message: "HIEF git hooks not installed — run `hief hooks install`".to_string(),
            fixable: true,
            fixed: false,
        };
        if fix {
            if let Ok(()) = hooks_install(project_root, false) {
                check.status = "ok".to_string();
                check.message = "Git hooks installed".to_string();
                check.fixed = true;
                fixes_applied += 1;
            }
        }
        checks.push(check);
    } else {
        checks.push(DoctorCheck {
            name: "git_hooks".to_string(),
            status: "ok".to_string(),
            message: "Git hooks installed".to_string(),
            fixable: false,
            fixed: false,
        });
    }

    let healthy = checks.iter().all(|c| c.status != "error");

    let report = DoctorReport {
        healthy,
        checks,
        fixes_applied,
    };

    if json {
        println!("{}", serde_json::to_string_pretty(&report).unwrap());
    } else {
        println!("🩺 HIEF Doctor Report\n");
        for check in &report.checks {
            let icon = match check.status.as_str() {
                "ok" => "✅",
                "warning" => "⚠️ ",
                "error" => "❌",
                _ => "❓",
            };
            let fixed_tag = if check.fixed { " (FIXED)" } else { "" };
            println!("  {} {} — {}{}", icon, check.name, check.message, fixed_tag);
        }
        println!();
        if report.healthy {
            println!("✅ Overall: healthy");
        } else {
            println!("❌ Overall: issues detected");
        }
        if report.fixes_applied > 0 {
            println!("🔧 {} fixes applied", report.fixes_applied);
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Hooks commands
// ---------------------------------------------------------------------------

const POST_COMMIT_HOOK: &str = r#"#!/bin/sh
# HIEF auto-index hook — installed by `hief hooks install`
# Incrementally updates the code index after every commit.
if command -v hief >/dev/null 2>&1; then
    hief index build --json >/dev/null 2>&1 &
fi
"#;

const PRE_PUSH_HOOK: &str = r#"#!/bin/sh
# HIEF pre-push eval check — installed by `hief hooks install`
# Runs evaluation checks before pushing to catch regressions.
if command -v hief >/dev/null 2>&1; then
    echo "🔍 Running HIEF evaluation checks..."
    hief eval run --ci
    exit_code=$?
    if [ $exit_code -ne 0 ]; then
        echo "❌ HIEF evaluation failed — push blocked"
        exit 1
    fi
fi
"#;

/// Install HIEF git hooks.
pub fn hooks_install(project_root: &Path, json: bool) -> Result<()> {
    let hooks_dir = project_root.join(".git/hooks");

    if !hooks_dir.exists() {
        if json {
            println!(
                "{}",
                serde_json::json!({"error": "Not a git repository — .git/hooks not found"})
            );
        } else {
            println!("❌ Not a git repository — .git/hooks not found");
        }
        return Ok(());
    }

    let mut installed = Vec::new();

    // Install post-commit hook
    let post_commit_path = hooks_dir.join("post-commit");
    install_hook(&post_commit_path, POST_COMMIT_HOOK, "post-commit", &mut installed)?;

    // Install pre-push hook
    let pre_push_path = hooks_dir.join("pre-push");
    install_hook(&pre_push_path, PRE_PUSH_HOOK, "pre-push", &mut installed)?;

    if json {
        println!(
            "{}",
            serde_json::json!({"installed": installed, "hooks_dir": hooks_dir.display().to_string()})
        );
    } else {
        if installed.is_empty() {
            println!("⏭️  All hooks already installed");
        } else {
            for hook_name in &installed {
                println!("✅ Installed {} hook", hook_name);
            }
        }
        println!("   Hooks directory: {}", hooks_dir.display());
    }

    Ok(())
}

/// Uninstall HIEF git hooks.
pub fn hooks_uninstall(project_root: &Path, json: bool) -> Result<()> {
    let hooks_dir = project_root.join(".git/hooks");
    let mut removed = Vec::new();

    for hook_name in &["post-commit", "pre-push"] {
        let hook_path = hooks_dir.join(hook_name);
        if hook_path.exists() {
            let content = std::fs::read_to_string(&hook_path).unwrap_or_default();
            if content.contains("hief") {
                std::fs::remove_file(&hook_path)?;
                removed.push(hook_name.to_string());
            }
        }
    }

    if json {
        println!("{}", serde_json::json!({"removed": removed}));
    } else if removed.is_empty() {
        println!("⏭️  No HIEF hooks found to remove");
    } else {
        for hook_name in &removed {
            println!("🗑️  Removed {} hook", hook_name);
        }
    }

    Ok(())
}

/// Show git hook status.
pub fn hooks_status(project_root: &Path, json: bool) -> Result<()> {
    let hooks_dir = project_root.join(".git/hooks");

    let hook_names = ["post-commit", "pre-push"];
    let mut statuses = Vec::new();

    for hook_name in &hook_names {
        let hook_path = hooks_dir.join(hook_name);
        let installed = if hook_path.exists() {
            let content = std::fs::read_to_string(&hook_path).unwrap_or_default();
            content.contains("hief")
        } else {
            false
        };
        statuses.push(serde_json::json!({
            "hook": hook_name,
            "installed": installed,
        }));
    }

    if json {
        println!("{}", serde_json::to_string_pretty(&statuses).unwrap());
    } else {
        println!("🪝 Git Hook Status:\n");
        for s in &statuses {
            let name = s["hook"].as_str().unwrap();
            let installed = s["installed"].as_bool().unwrap();
            let icon = if installed { "✅" } else { "❌" };
            println!("  {} {} — {}", icon, name, if installed { "installed" } else { "not installed" });
        }
    }

    Ok(())
}

/// Helper: install a single hook, appending to existing if needed.
fn install_hook(
    path: &Path,
    content: &str,
    name: &str,
    installed: &mut Vec<String>,
) -> Result<()> {
    if path.exists() {
        let existing = std::fs::read_to_string(path)?;
        if existing.contains("hief") {
            return Ok(()); // Already installed
        }
        // Append to existing hook
        std::fs::write(path, format!("{}\n{}", existing.trim(), content))?;
    } else {
        std::fs::write(path, content)?;
    }

    // Make executable
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o755);
        std::fs::set_permissions(path, perms)?;
    }

    installed.push(name.to_string());
    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn status_icon(status: &str) -> &'static str {
    match status {
        "draft" => "📝",
        "approved" => "✅",
        "in_progress" => "🔨",
        "in_review" => "👀",
        "verified" => "✔️",
        "merged" => "🎉",
        "rejected" => "❌",
        "blocked" => "🔒",
        _ => "❓",
    }
}

const DEFAULT_HIEFIGNORE: &str = r#"# HIEF ignore patterns (in addition to .gitignore)
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

const DEFAULT_AGENTS_MD: &str = r#"# AGENTS.md

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
