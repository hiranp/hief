//! `hief skills` — manage skill files used by agents.

use std::path::Path;

use crate::config::Config;
use crate::errors::Result;

/// Initialize skills directory structure.
pub fn skills_init(project_root: &Path, config: &Config, json: bool) -> Result<()> {
    let report = crate::skills::scaffold_skills_dir(project_root, &config.skills)?;

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&report).expect("failed to serialize report")
        );
    } else {
        for dir in &report.directories_created {
            println!("✅ Created {}", dir);
        }
        for file in &report.files_created {
            println!("✅ Created {}", file);
        }
        for item in &report.already_existed {
            println!("⏭️  {} already exists", item);
        }
        println!(
            "\n📁 Skills directory is ready. Add .md recipe files under {}.",
            config.skills.skills_path
        );
        println!("   Skills added here will appear to agents instantly.");
        println!(
            "   Tip: call the `reload_skills` MCP tool to hot-reload recipes in a running server."
        );
    }
    Ok(())
}

/// List available skills.
pub fn skills_list(project_root: &Path, config: &Config, json: bool) -> Result<()> {
    let names = crate::skills::list_skills(project_root, &config.skills)?;
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&names).expect("failed to serialize names")
        );
    } else {
        println!("📋 Skills ({}):", names.len());
        for n in names {
            println!("  {}", n);
        }
    }
    Ok(())
}

/// Show contents of a skill.
pub fn skills_show(project_root: &Path, config: &Config, name: &str, json: bool) -> Result<()> {
    let content = crate::skills::get_skill(project_root, &config.skills, name)?;
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&content).expect("failed to serialize content")
        );
    } else {
        println!("--- {} ---\n{}", name, content);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use tempfile::tempdir;

    #[test]
    fn cli_skills_end_to_end() {
        let root = tempdir().expect("failed to create tempdir");
        let config_path = root.path().join("hief.toml");
        Config::write_default(&config_path).expect("failed to write default config");
        let config = Config::load(&config_path).expect("failed to load config");

        // init should create skills directory, README, and the default hief_protocol skill
        skills_init(root.path(), &config, false).expect("skills_init failed");
        let skills_dir = root.path().join(&config.skills.skills_path);
        assert!(skills_dir.exists());
        assert!(skills_dir.join("README.md").exists());
        assert!(skills_dir.join("hief_protocol.md").exists());

        // after init, hief_protocol is the only non-README skill listed
        let names =
            crate::skills::list_skills(root.path(), &config.skills).expect("list_skills failed");
        assert!(names.contains(&"hief_protocol".to_string()));

        // add a custom skill and ensure it appears alongside hief_protocol
        std::fs::write(skills_dir.join("foo.md"), "bar").expect("failed to write foo skill");
        let names =
            crate::skills::list_skills(root.path(), &config.skills).expect("list_skills failed");
        assert!(names.contains(&"foo".to_string()));
        assert!(names.contains(&"hief_protocol".to_string()));
        skills_list(root.path(), &config, false).expect("skills_list failed");

        let content =
            crate::skills::get_skill(root.path(), &config.skills, "foo").expect("get_skill failed");
        assert_eq!(content, "bar");
        skills_show(root.path(), &config, "foo", false).expect("skills_show failed");
    }
}
