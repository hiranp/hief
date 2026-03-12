//! Dynamic MCP tool support for `.hief/skills` markdown recipes.
//!
//! Skills are simple markdown/yaml files stored in the project's
//! `skills_path` (typically `.hief/skills/`) that describe step‑by‑step
//! procedures or project‑specific workflows. Each file is automatically
//! published as an MCP tool whose invocation returns the raw text of the file.
//!
//! This module provides a loader, an in‑memory registry, and helpers to
//! convert skills into valid MCP tool definitions. The HIEF server loads the
//! registry at startup and adds wildcard routes for `execute_skill_*` tools.

use anyhow::Result;
use serde_json::json;
use rmcp::model::{Tool, JsonObject};
use std::{collections::HashMap, path::Path, sync::{Arc, RwLock}};


/// A parsed skill file.
#[derive(Debug, Clone)]
pub struct Skill {
    pub name: String,        // tool name (execute_skill_...)
    pub description: String, // short human description
    pub content: String,     // entire file text
}

/// Registry of skills loaded from disk.  Internally stored in an `Arc<RwLock<...>>`
/// so it can be cheaply cloned and updated in place.
#[derive(Default, Clone)]
pub struct SkillRegistry {
    inner: Arc<RwLock<HashMap<String, Skill>>>,
}

impl SkillRegistry {
    /// Construct an empty registry.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Load all markdown skills from the given project root, replacing any
    /// existing entries.
    pub fn load_from_disk(&self, project_root: &Path) -> Result<()> {
        let skills = load_skills_from_disk(project_root)?;
        let mut map = self.inner.write().unwrap();
        map.clear();
        for skill in skills {
            map.insert(skill.name.clone(), skill);
        }
        Ok(())
    }

    /// Retrieve a skill by its tool name (e.g. `execute_skill_foo`).
    pub fn by_tool(&self, tool: &str) -> Option<Skill> {
        self.inner.read().unwrap().get(tool).cloned()
    }

    /// Generate a list of MCP tool schema objects suitable for broadcasting
    /// in `ServerInfo` or registering with the router.
    pub fn tool_defs(&self) -> Vec<Tool> {
        let skills: Vec<Skill> = self.inner.read().unwrap().values().cloned().collect();
        generate_skill_tool_definitions(&skills)
    }
}

// -- loader helpers --------------------------------------------------------

/// Scan the skills directory and return a vector of parsed `Skill` values.
fn load_skills_from_disk(project_root: &Path) -> Result<Vec<Skill>> {
    let config = crate::config::Config::load(&project_root.join("hief.toml"))?;
    let mut skills_vec = Vec::new();

    let list = crate::skills::list_skills(project_root, &config.skills)?;
    for name in list {
        // read raw text and derive description
        let content = crate::skills::get_skill(project_root, &config.skills, &name)?;
        let first_line = content
            .lines()
            .next()
            .unwrap_or("Custom project skill.")
            .trim_start_matches('#')
            .trim()
            .to_string();
        let description = format!("Instructions for: {}", first_line);
        let tool_name = format!("execute_skill_{}", name.replace('-', "_"));
        skills_vec.push(Skill {
            name: tool_name,
            description,
            content,
        });
    }

    Ok(skills_vec)
}

/// Convert a series of skills into MCP `Tool` values that can be registered
/// with the router.  This avoids manual JSON assembly and keeps every tool
/// definition in‑sync with the actual skill metadata.
pub fn generate_skill_tool_definitions(skills: &[Skill]) -> Vec<Tool> {
    skills
        .iter()
        .map(|skill| {
            let input_schema: JsonObject = json!({
                "type": "object",
                "properties": {
                    "reason": {
                        "type": "string",
                        "description": "Briefly explain why you are invoking this skill."
                    }
                },
                "required": ["reason"]
            })
            .as_object()
            .cloned()
            .unwrap_or_default();

            Tool {
                name: skill.name.clone().into(),
                title: None,
                description: Some(skill.description.clone().into()),
                input_schema: input_schema.into(),
                output_schema: None,
                annotations: None,
                execution: None,
                icons: None,
                meta: None,
            }
        })
        .collect()
}

// -- tests ------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use crate::config::SkillsConfig;
    use crate::skills;

    #[test]
    fn registry_load_and_query() {
        let tmp = tempdir().unwrap();
        let config = SkillsConfig::default();
        skills::scaffold_skills_dir(tmp.path(), &config).unwrap();
        std::fs::write(
            tmp.path().join(&config.skills_path).join("foo.md"),
            "# Foo skill\ndo something",
        )
        .unwrap();

        let reg = SkillRegistry::new();
        reg.load_from_disk(tmp.path()).unwrap();
        let defs = reg.tool_defs();
        assert_eq!(defs.len(), 1);
        assert!(defs[0].name.starts_with("execute_skill_foo"));

        let opt = reg.by_tool("execute_skill_foo");
        assert!(opt.is_some());
        assert!(opt.unwrap().content.contains("do something"));
    }
}
