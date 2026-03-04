//! `hief docs` — documentation scaffolding and template commands.

use std::path::Path;

use crate::config::Config;
use crate::errors::Result;

/// Initialize docs directory structure.
pub fn docs_init(project_root: &Path, config: &Config, json: bool) -> Result<()> {
    let report = crate::docs::scaffold_docs_dirs(project_root, &config.docs)?;

    if json {
        println!("{}", serde_json::to_string_pretty(&report).unwrap());
    } else {
        for dir in &report.directories_created {
            println!("✅ Created {}", dir);
        }
        for file in &report.files_created {
            println!("✅ Created {}", file);
        }
        for file in &report.files_updated {
            println!("♻️  Updated {}", file);
        }
        for item in &report.already_existed {
            println!("⏭️  {} already exists", item);
        }
        println!(
            "\n📁 Docs structure ready. Custom templates go in: {}",
            report.templates_dir
        );
        println!("   Run `hief docs list` to see available templates.");

        if report.prompt_created {
            println!(
                "\n🤖 SDD LLM Prompt generated: {}/SDD_LLM_PROMPT.md",
                report.templates_dir
            );
            println!(
                "   Copy & paste its contents to your AI assistant to effortlessly draft detailed SDD documents."
            );
            println!(
                "   Check {}/frameworks/ for language-specific rules.",
                report.templates_dir
            );
        }
    }

    Ok(())
}

/// Generate a document from a template.
pub async fn docs_generate(
    project_root: &Path,
    config: &Config,
    template: &str,
    name: Option<&str>,
    id: Option<&str>,
    output: Option<&str>,
    vars: &[String],
    auto_populate: bool,
    force: bool,
    json: bool,
) -> Result<()> {
    use std::collections::HashMap;

    // Validate template exists
    let meta = crate::docs::templates::get_template_meta(template).ok_or_else(|| {
        crate::errors::HiefError::Other(format!(
            "unknown template '{}'. Run `hief docs list` to see available templates.",
            template
        ))
    })?;

    // Build variable map from CLI flags
    let mut variables = HashMap::new();

    if let Some(n) = name {
        // Set the appropriate variable based on template type
        match template {
            "spec" | "harness" => {
                variables.insert("feature".to_string(), n.to_string());
            }
            "playbook" => {
                variables.insert("scenario".to_string(), n.to_string());
            }
            "golden" => {
                variables.insert("name".to_string(), n.to_string());
            }
            _ => {
                // For other templates, try setting name as a generic variable
                variables.insert("name".to_string(), n.to_string());
            }
        }
    }

    if let Some(intent_id) = id {
        variables.insert("id".to_string(), intent_id.to_string());
    }

    // Parse --var KEY=VALUE pairs
    for var_str in vars {
        if let Some((key, value)) = var_str.split_once('=') {
            variables.insert(key.trim().to_string(), value.trim().to_string());
        } else {
            return Err(crate::errors::HiefError::Other(format!(
                "invalid variable format '{}'. Expected KEY=VALUE",
                var_str
            )));
        }
    }

    // Auto-populate from project context (Cargo.toml, git, etc.)
    variables = crate::docs::auto_populate_variables(project_root, &variables);

    // Auto-populate from index if requested
    if auto_populate {
        let db_path = Config::db_path(project_root);
        if db_path.exists() {
            let db = crate::db::Database::open(&db_path).await?;
            crate::docs::auto_populate_from_index(&db, project_root, &mut variables).await?;
        } else if !json {
            println!(
                "⚠️  Database not found — skipping index auto-populate. Run `hief init` first."
            );
        }
    }

    // Resolve template content (file override or embedded)
    let template_content = crate::docs::resolve_template(project_root, template)?;

    // Render with variables and normalize output for stable files across OSes
    let rendered = crate::docs::render_template(&template_content, &variables);
    let normalized_rendered = crate::docs::normalize_generated_document(&rendered);

    // Resolve output path
    let output_path = if template == "golden" {
        if let Some(explicit) = output {
            project_root.join(explicit)
        } else {
            let name = variables.get("name").map(|v| v.trim()).unwrap_or("");
            if name.is_empty() {
                return Err(crate::errors::HiefError::Other(
                    "template 'golden' requires --name <set-name> (or --var name=<set-name>)"
                        .to_string(),
                ));
            }

            project_root
                .join(&config.eval.golden_set_path)
                .join(format!("{}.toml", name))
        }
    } else {
        crate::docs::resolve_output_path(project_root, &config.docs, template, &variables, output)
    };

    let output_display = output_path.display().to_string();
    if output_display.contains("{{") || output_display.contains("}}") {
        return Err(crate::errors::HiefError::Other(format!(
            "resolved output path contains unresolved placeholders: {}. Provide required variables (e.g. --name)",
            output_display
        )));
    }

    // Check if file exists and not forced
    if output_path.exists() && !force {
        if json {
            println!(
                "{}",
                serde_json::json!({
                    "error": "file_exists",
                    "path": output_path.display().to_string(),
                    "hint": "Use --force to overwrite"
                })
            );
            return Ok(());
        }
        return Err(crate::errors::HiefError::Other(format!(
            "file already exists: {}. Use --force to overwrite.",
            output_path.display()
        )));
    }

    // Create parent directories if needed
    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // Write the rendered document
    std::fs::write(&output_path, &normalized_rendered)?;

    let unresolved = crate::docs::count_unresolved(&normalized_rendered);
    let mut resolved_vars: Vec<_> = variables.keys().cloned().collect();
    resolved_vars.sort();

    if json {
        println!(
            "{}",
            serde_json::json!({
                "template": template,
                "output": output_path.display().to_string(),
                "variables_resolved": resolved_vars,
                "placeholders_remaining": unresolved,
            })
        );
    } else {
        println!("✅ Generated {} → {}", meta.name, output_path.display());
        if !resolved_vars.is_empty() {
            println!("   Variables resolved: {}", resolved_vars.join(", "));
        }
        if unresolved > 0 {
            println!(
                "   ⚠️  {} placeholder{} remaining — edit the file to fill them in.",
                unresolved,
                if unresolved == 1 { "" } else { "s" }
            );
        }
    }

    Ok(())
}

/// List available templates.
pub fn docs_list(json: bool) -> Result<()> {
    use crate::docs::templates::TEMPLATES;

    if json {
        let items: Vec<serde_json::Value> = TEMPLATES
            .iter()
            .map(|t| {
                serde_json::json!({
                    "id": t.id,
                    "name": t.name,
                    "description": t.description,
                    "category": t.category,
                    "default_output": t.default_output,
                    "variables": t.variables,
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&items).unwrap());
    } else {
        println!("📋 Available Templates:\n");
        let mut current_category = "";
        for t in TEMPLATES {
            if t.category != current_category {
                current_category = t.category;
                let label = match current_category {
                    "spec" => "📝 SDD (Spec-Driven Development)",
                    "harness" => "🧪 HDD (Harness-Driven Development)",
                    "golden" => "🏆 Evaluation",
                    _ => current_category,
                };
                println!("  {}\n", label);
            }
            println!("    {} — {}", t.id, t.description);
            println!(
                "      Output: {}  |  Variables: {}",
                t.default_output,
                t.variables.join(", ")
            );
            println!();
        }
        println!("Usage: hief docs generate <template> [--name <name>] [--var KEY=VALUE]");
    }

    Ok(())
}

/// Check docs directory structure.
pub fn docs_check(project_root: &Path, config: &Config, json: bool) -> Result<()> {
    let report = crate::docs::check_docs_structure(project_root, &config.docs);

    if json {
        println!("{}", serde_json::to_string_pretty(&report).unwrap());
    } else {
        println!("📁 Docs Structure Check\n");
        for check in &report.checks {
            let icon = match check.status.as_str() {
                "ok" => "✅",
                "missing" => "❌",
                "warning" => "⚠️ ",
                _ => "❓",
            };
            println!("  {} {} — {}", icon, check.name, check.message);
        }
        println!();
        if report.healthy {
            println!("✅ Docs structure is healthy");
        } else {
            println!("❌ Docs structure has issues — run `hief docs init` to fix");
        }
    }

    Ok(())
}

/// Show variables for a template.
pub fn docs_variables(template: &str, json: bool) -> Result<()> {
    let meta = crate::docs::templates::get_template_meta(template).ok_or_else(|| {
        crate::errors::HiefError::Other(format!(
            "unknown template '{}'. Run `hief docs list` to see available templates.",
            template
        ))
    })?;

    // Get the template content and extract all variables
    let content = crate::docs::templates::get_template_content(template).unwrap_or("");
    let all_vars = crate::docs::extract_variables(content);

    if json {
        println!(
            "{}",
            serde_json::json!({
                "template": template,
                "name": meta.name,
                "primary_variables": meta.variables,
                "all_variables": all_vars,
                "default_output": meta.default_output,
            })
        );
    } else {
        println!("📋 Template: {} ({})\n", meta.name, meta.id);
        println!("  Primary variables (set via CLI flags):");
        for v in meta.variables {
            let flag = match *v {
                "feature" | "scenario" | "name" => "--name <value>".to_string(),
                "id" => "--id <value>".to_string(),
                _ => format!("--var {}=<value>", v),
            };
            println!("    {{{{{}}}}} → {}", v, flag);
        }
        println!("\n  All variables in template ({}):", all_vars.len());
        for v in &all_vars {
            println!("    {{{{{}}}}}", v);
        }
        println!("\n  Auto-populated:");
        println!("    {{{{project_name}}}} — from Cargo.toml / package.json / git remote");
        println!(
            "    {{{{file_count}}}}, {{{{chunk_count}}}}, {{{{languages}}}} — with --auto-populate"
        );
        println!("\n  Output: {}", meta.default_output);
    }

    Ok(())
}
