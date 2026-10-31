//! `hief skills export` — write hief workflows as agent skill files.
//!
//! MCP tools cost context in every session, loaded or not. A skill file
//! (`<dir>/<name>/SKILL.md`) costs context only when the agent loads it. This
//! exports the less common workflows, plus the project's own `.hief/skills`,
//! so they can be dropped from `[mcp] tools`.

use std::path::{Path, PathBuf};

use clap::ValueEnum;

use super::list_skills;
use crate::config::SkillsConfig;
use crate::errors::{HiefError, Result};

/// Agent harness to export for. All three load `<dir>/<name>/SKILL.md`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum SkillTarget {
    Claude,
    Opencode,
    Gemini,
}

impl SkillTarget {
    /// Default output directory, relative to the project root.
    pub fn dir(self) -> &'static str {
        match self {
            SkillTarget::Claude => ".claude/skills",
            SkillTarget::Opencode => ".opencode/skills",
            SkillTarget::Gemini => ".gemini/skills",
        }
    }
}

/// One skill ready to write.
pub struct SkillFile {
    pub name: String,
    pub description: String,
    pub body: String,
}

/// What an export did.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct ExportReport {
    pub written: Vec<PathBuf>,
    pub skipped: Vec<PathBuf>,
}

const BUILTIN: &[(&str, &str, &str)] = &[
    (
        "hief-intents",
        "Track tasks as an intent graph with dependencies, using the hief CLI. Use when planning multi-step work, picking the next task, or updating task status.",
        "Create, order and update tasks.\n\n```sh\nhief graph create --kind feature --title \"Add X\" --depends-on <id>\nhief graph list --status approved\nhief graph ready              # intents whose dependencies are done\nhief graph show <id>\nhief graph update <id> --status in_progress --assign <agent>\nhief graph validate           # cycles, orphans, blocked nodes\n```\n\nSet the status to `in_review` when the work is done. Never approve your own intent.\n",
    ),
    (
        "hief-eval",
        "Run golden-set evaluations and read score history with the hief CLI. Use after a change, before merging, or when checking for quality regressions.",
        "```sh\nhief eval golden               # list golden sets\nhief eval run                  # run all golden sets\nhief eval run --golden <name>  # run one\nhief eval run --ci             # enforce thresholds; exit 1 on failure\nhief eval report <name>        # score history and trend\n```\n\nA golden-set regression blocks the merge.\n",
    ),
    (
        "hief-drift",
        "Check that the .hief scaffold and agent docs still match the code, and gate CI on new drift. Use after editing docs, AGENTS.md, or conventions.",
        "```sh\nhief check                          # 0-100 drift score and issues\nhief check --diff-base origin/main  # only files changed since main\nhief check --write-baseline         # accept today's drift; fail only on new\nhief check --format github          # CI annotations\nhief sync --backend none            # print a targeted repair prompt\n```\n\nExit codes: 0 clean, 1 new errors, 2 usage or config error.\n",
    ),
    (
        "hief-patterns",
        "Read and write project-specific how-to patterns in .hief/patterns using the hief CLI. Use when a task matches a recurring procedure or you learn one worth saving.",
        "```sh\nhief patterns list\nhief patterns show <name>\nhief patterns create <name> --title \"Title\" --content \"# Steps...\"\nhief patterns sync   # regenerate .hief/patterns/INDEX.md\n```\n",
    ),
    (
        "hief-docs",
        "Scaffold and check project docs from templates with the hief CLI. Use when adding specs, ADRs or harness docs.",
        "```sh\nhief docs init\nhief docs list\nhief docs variables <template>\nhief docs generate <template> --name \"Title\"\nhief docs check\n```\n",
    ),
    (
        "hief-claims",
        "Claim an issue so parallel agents in other worktrees do not take it. Use before starting work on an issue and when finishing it.",
        "```sh\nhief claim <issue> --lease 4h --agent <you>   # fails if someone holds a live claim\nhief claims                                   # who holds what, with expiry\nhief release <issue> --agent <you>            # only the holder, unless --force\n```\n\nAdd `--github` to also label the issue `status:in-progress`.\n",
    ),
];

/// The built-in workflow skills.
pub fn builtin() -> Vec<SkillFile> {
    BUILTIN
        .iter()
        .map(|(name, description, body)| SkillFile {
            name: (*name).to_string(),
            description: (*description).to_string(),
            body: format!("# {name}\n\n{body}"),
        })
        .collect()
}

/// Lowercase `[a-z0-9-]` name, as skill loaders require.
fn skill_name(stem: &str) -> String {
    let cleaned: String = stem
        .to_ascii_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let cleaned = cleaned.trim_matches('-').to_string();
    if cleaned.starts_with("hief") {
        cleaned
    } else {
        format!("hief-{cleaned}")
    }
}

fn first_line_description(text: &str) -> String {
    let line = text
        .lines()
        .map(|l| l.trim().trim_start_matches('#').trim())
        .find(|l| !l.is_empty() && *l != "---")
        .unwrap_or("hief skill");
    let line: String = line.chars().take(200).collect();
    line.replace(['\n', '\r'], " ")
}

/// Built-in skills plus the project's own `.hief/skills` recipes.
pub fn collect(project_root: &Path, config: &SkillsConfig) -> Result<Vec<SkillFile>> {
    let mut skills = builtin();
    let dir = project_root.join(&config.skills_path);
    let mut names = list_skills(project_root, config)?;
    names.sort();
    for stem in names {
        let path = ["md", "yaml", "yml"]
            .iter()
            .map(|e| dir.join(format!("{stem}.{e}")))
            .find(|p| p.is_file());
        let Some(path) = path else { continue };
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let name = skill_name(&stem);
        if name == "hief-" || skills.iter().any(|s| s.name == name) {
            continue;
        }
        let is_md = path.extension().is_some_and(|e| e == "md");
        skills.push(SkillFile {
            description: first_line_description(&text),
            body: if is_md {
                text
            } else {
                format!("```yaml\n{text}\n```\n")
            },
            name,
        });
    }
    Ok(skills)
}

fn render(skill: &SkillFile) -> String {
    let desc = skill.description.replace('"', "'");
    // A recipe that already has front matter keeps it, so it is not nested twice.
    let body = if skill.body.starts_with("---\n") {
        skill.body.clone()
    } else {
        format!(
            "---\nname: {}\ndescription: \"{desc}\"\n---\n\n{}",
            skill.name, skill.body
        )
    };
    if body.ends_with('\n') {
        body
    } else {
        body + "\n"
    }
}

/// Write skills to `<out or target dir>/<name>/SKILL.md`. Existing files are
/// kept unless `force`.
pub fn export(
    project_root: &Path,
    config: &SkillsConfig,
    target: SkillTarget,
    out: Option<&Path>,
    force: bool,
) -> Result<ExportReport> {
    let base = out.map_or_else(|| project_root.join(target.dir()), Path::to_path_buf);
    let mut report = ExportReport::default();
    for skill in collect(project_root, config)? {
        let dir = base.join(&skill.name);
        let path = dir.join("SKILL.md");
        if path.exists() && !force {
            report.skipped.push(path);
            continue;
        }
        std::fs::create_dir_all(&dir)
            .map_err(|e| HiefError::Other(format!("cannot create {}: {e}", dir.display())))?;
        std::fs::write(&path, render(&skill))?;
        report.written.push(path);
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> SkillsConfig {
        SkillsConfig {
            skills_path: ".hief/skills".to_string(),
        }
    }

    #[test]
    fn builtin_skills_have_valid_names_and_one_line_descriptions() {
        for s in builtin() {
            assert!(
                s.name
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
                "{}",
                s.name
            );
            assert!(!s.description.contains('\n') && s.description.len() < 1024);
            let text = render(&s);
            assert!(text.starts_with(&format!("---\nname: {}\n", s.name)));
            assert!(
                text.matches("\n---\n").count() == 1,
                "one front-matter block"
            );
        }
    }

    #[test]
    fn every_cli_command_in_builtin_skills_exists() {
        use clap::CommandFactory;
        let root = crate::cli::Cli::command();
        for s in builtin() {
            for line in s.body.lines().filter(|l| l.starts_with("hief ")) {
                let mut cmd = &root;
                for word in line
                    .split_whitespace()
                    .skip(1)
                    .take_while(|w| !w.starts_with('-') && !w.starts_with('<'))
                {
                    match cmd.find_subcommand(word) {
                        Some(sub) => cmd = sub,
                        None => break,
                    }
                }
                assert_ne!(
                    cmd.get_name(),
                    "hief",
                    "no such command in skill {}: {line}",
                    s.name
                );
            }
        }
    }

    #[test]
    fn export_writes_builtin_and_project_skills_and_keeps_existing() {
        let dir = tempfile::tempdir().unwrap();
        let skills = dir.path().join(".hief/skills");
        std::fs::create_dir_all(&skills).unwrap();
        std::fs::write(
            skills.join("db_migration.md"),
            "# Create a DB migration\n\nSteps...",
        )
        .unwrap();
        std::fs::write(skills.join("README.md"), "ignored").unwrap();

        let report = export(dir.path(), &cfg(), SkillTarget::Claude, None, false).unwrap();
        assert_eq!(report.written.len(), BUILTIN.len() + 1);
        let migration = dir.path().join(".claude/skills/hief-db-migration/SKILL.md");
        let text = std::fs::read_to_string(&migration).unwrap();
        assert!(text.contains("name: hief-db-migration"));
        assert!(text.contains("description: \"Create a DB migration\""));

        std::fs::write(&migration, "mine").unwrap();
        let again = export(dir.path(), &cfg(), SkillTarget::Claude, None, false).unwrap();
        assert!(again.written.is_empty() && again.skipped.len() == BUILTIN.len() + 1);
        assert_eq!(std::fs::read_to_string(&migration).unwrap(), "mine");
        export(dir.path(), &cfg(), SkillTarget::Claude, None, true).unwrap();
        assert!(
            std::fs::read_to_string(&migration)
                .unwrap()
                .contains("Steps...")
        );
    }

    #[test]
    fn target_dirs_differ() {
        assert_eq!(SkillTarget::Opencode.dir(), ".opencode/skills");
        assert_eq!(SkillTarget::Gemini.dir(), ".gemini/skills");
    }
}
