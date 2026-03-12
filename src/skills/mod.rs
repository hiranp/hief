//! Support for "skills" — executable conventions stored in .hief/skills.

use std::path::Path;

use serde::Serialize;

use crate::config::SkillsConfig;
use crate::errors::{HiefError, Result};

/// Report returned from `skills_init`.
#[derive(Serialize, Debug)]
pub struct SkillsInitReport {
    pub directories_created: Vec<String>,
    pub files_created: Vec<String>,
    pub already_existed: Vec<String>,
}

/// Ensure the skills directory exists and contains a README explaining its purpose.
pub fn scaffold_skills_dir(project_root: &Path, config: &SkillsConfig) -> Result<SkillsInitReport> {
    let mut report = SkillsInitReport {
        directories_created: Vec::new(),
        files_created: Vec::new(),
        already_existed: Vec::new(),
    };

    let skills_path = project_root.join(&config.skills_path);
    if skills_path.exists() {
        report
            .already_existed
            .push(config.skills_path.clone());
    } else {
        std::fs::create_dir_all(&skills_path)?;
        report
            .directories_created
            .push(config.skills_path.clone());
    }

    // README explaining what skills are
    let readme = skills_path.join("README.md");
    if readme.exists() {
        report
            .already_existed
            .push(format!("{}/README.md", config.skills_path));
    } else {
        const SKILLS_README: &str = r#"# Skills Directory

This folder is intended to contain *skills* or *recipes* which are small
markdown or YAML files describing how to perform common tasks in this
codebase. Each file should have a human-friendly name and can be referenced
by AI agents via the MCP server.

Example: `create_database_migration.md` might contain step-by-step commands
for adding a new migration file, running tests, and updating docs.
"#;
        std::fs::write(&readme, SKILLS_README)?;
        report
            .files_created
            .push(format!("{}/README.md", config.skills_path));
    }

    Ok(report)
}

/// List skill names (file stems) present in the skills directory.
pub fn list_skills(project_root: &Path, config: &SkillsConfig) -> Result<Vec<String>> {
    let mut names = Vec::new();
    let skills_path = project_root.join(&config.skills_path);
    if skills_path.is_dir() {
        for entry in std::fs::read_dir(&skills_path)? {
            let entry = entry?;
            if let Some(stem) = entry.path().file_stem().and_then(|s| s.to_str()) {
                if stem != "README" {
                    names.push(stem.to_string());
                }
            }
        }
    }
    Ok(names)
}

/// Return the contents of a named skill file (searches .md and .yaml).
///
/// # Security
/// `name` must contain only alphanumeric characters, underscores, and hyphens.
/// Path separators and dots are rejected to prevent path traversal.
pub fn get_skill(project_root: &Path, config: &SkillsConfig, name: &str) -> Result<String> {
    // Validate: only allow alphanumeric, underscores, hyphens; no dots, slashes, or leading hyphens.
    if name.is_empty()
        || name.starts_with('-')
        || !name.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-')
    {
        return Err(HiefError::SecurityViolation(format!(
            "Invalid skill name '{}': only alphanumeric characters, underscores, and hyphens are allowed",
            name
        )));
    }

    let skills_path = project_root.join(&config.skills_path);
    let candidates = ["md", "yaml", "yml", "txt"];
    for ext in &candidates {
        let path = skills_path.join(format!("{}.{}", name, ext));
        if path.exists() {
            return std::fs::read_to_string(&path).map_err(HiefError::Io);
        }
    }
    Err(HiefError::Other(format!("skill not found: {}", name)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::SkillsConfig;
    use tempfile::tempdir;

    #[test]
    fn test_scaffold_skills_dir_creates_structure() {
        let dir = tempdir().unwrap();
        let config = SkillsConfig::default();
        let report = scaffold_skills_dir(dir.path(), &config).unwrap();
        assert!(report.directories_created.contains(&config.skills_path));
        assert!(report.files_created.contains(&format!("{}/README.md", config.skills_path)));

        // running again should mark as already_existed
        let report2 = scaffold_skills_dir(dir.path(), &config).unwrap();
        assert!(report2.already_existed.contains(&config.skills_path));
        assert!(report2.already_existed.contains(&format!("{}/README.md", config.skills_path)));
    }

    #[test]
    fn test_list_and_get_skill() {
        let dir = tempdir().unwrap();
        let config = SkillsConfig::default();
        scaffold_skills_dir(dir.path(), &config).unwrap();
        let skill_path = dir
            .path()
            .join(&config.skills_path)
            .join("foo.md");
        std::fs::write(&skill_path, "hello").unwrap();

        let names = list_skills(dir.path(), &config).unwrap();
        assert_eq!(names, vec!["foo".to_string()]);
        let content = get_skill(dir.path(), &config, "foo").unwrap();
        assert_eq!(content, "hello");
    }
}
