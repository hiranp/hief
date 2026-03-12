//! `hief skills` — manage skill files used by agents.

use std::path::Path;

use crate::config::Config;
use crate::errors::Result;

/// Initialize skills directory structure.
pub fn skills_init(project_root: &Path, config: &Config, json: bool) -> Result<()> {
    let report = crate::skills::scaffold_skills_dir(project_root, &config.skills)?;

    if json {
        println!("{}", serde_json::to_string_pretty(&report).unwrap());
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
        println!("{}", serde_json::to_string_pretty(&names).unwrap());
    } else {
        println!("📋 Skills ({}):", names.len());
        for n in names {
            println!("  {}", n);
        }
    }
    Ok(())
}

/// Show contents of a skill.
pub fn skills_show(
    project_root: &Path,
    config: &Config,
    name: &str,
    json: bool,
) -> Result<()> {
    let content = crate::skills::get_skill(project_root, &config.skills, name)?;
    if json {
        println!("{}", serde_json::to_string_pretty(&content).unwrap());
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
        let root = tempdir().unwrap();
        let config_path = root.path().join("hief.toml");
        Config::write_default(&config_path).unwrap();
        let config = Config::load(&config_path).unwrap();

        // init should create skills directory and README
        skills_init(root.path(), &config, false).unwrap();
        let skills_dir = root.path().join(&config.skills.skills_path);
        assert!(skills_dir.exists());
        assert!(skills_dir.join("README.md").exists());

        // initially list returns empty vector
        let names = crate::skills::list_skills(root.path(), &config.skills).unwrap();
        assert!(names.is_empty());

        // add a dummy skill file and ensure it's listed
        std::fs::write(skills_dir.join("foo.md"), "bar").unwrap();
        let names = crate::skills::list_skills(root.path(), &config.skills).unwrap();
        assert_eq!(names, vec!["foo".to_string()]);
        skills_list(root.path(), &config, false).unwrap();

        let content = crate::skills::get_skill(root.path(), &config.skills, "foo").unwrap();
        assert_eq!(content, "bar");
        skills_show(root.path(), &config, "foo", false).unwrap();
    }
}
