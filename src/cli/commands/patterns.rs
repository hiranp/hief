//! `hief patterns` — project-scoped pattern library CLI commands.

use std::path::Path;

use crate::errors::Result;

/// List all patterns in .hief/patterns/.
pub fn patterns_list(project_root: &Path, json: bool) -> Result<()> {
    let patterns = crate::patterns::list_patterns(project_root);

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&patterns).unwrap_or_default()
        );
        return Ok(());
    }

    if patterns.is_empty() {
        println!("No patterns found in .hief/patterns/");
        println!("Create one with: hief patterns create <name>");
        return Ok(());
    }

    println!("\n  Project Patterns (.hief/patterns/)\n");
    for p in &patterns {
        println!("  {:30}  {}", p.name, p.title);
    }
    println!("\n  {} pattern(s) total", patterns.len());
    Ok(())
}

/// Show a single pattern's full content.
pub fn patterns_show(project_root: &Path, name: &str, json: bool) -> Result<()> {
    let content = crate::patterns::get_pattern(project_root, name)?;

    if json {
        let obj = serde_json::json!({ "name": name, "content": content });
        println!("{}", serde_json::to_string_pretty(&obj).unwrap_or_default());
    } else {
        println!("{}", content);
    }
    Ok(())
}

/// Create or update a pattern file.
pub fn patterns_create(
    project_root: &Path,
    name: &str,
    title: Option<&str>,
    inline_content: Option<&str>,
    json: bool,
) -> Result<()> {
    let content = if let Some(c) = inline_content {
        c.to_string()
    } else {
        // Build a starter template from the title
        let heading = title.unwrap_or(name);
        format!(
            "# Pattern: {}\n\n## Steps\n\n1. \n\n## Gotchas\n\n- \n\n## Verify\n\n- [ ] \n",
            heading
        )
    };

    crate::patterns::create_pattern(project_root, name, &content)?;

    if json {
        println!(
            "{}",
            serde_json::json!({ "created": true, "name": name, "path": format!(".hief/patterns/{}.md", name) })
        );
    } else {
        println!("✅ Created .hief/patterns/{}.md", name);
        println!("   INDEX.md updated automatically.");
    }
    Ok(())
}

/// Regenerate INDEX.md from files on disk.
pub fn patterns_sync(project_root: &Path, json: bool) -> Result<()> {
    crate::patterns::sync_index(project_root)?;

    if json {
        println!("{}", serde_json::json!({ "synced": true }));
    } else {
        println!("✅ .hief/patterns/INDEX.md regenerated");
    }
    Ok(())
}
