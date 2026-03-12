//! Embedded template strings for SDD/HDD document scaffolding.
//!
//! Each template uses `{{variable_name}}` syntax for substitution.
//! Unresolved variables remain as placeholders for the user/agent to fill in.

/// Template metadata for display and selection.
#[derive(Debug, Clone)]
pub struct TemplateMeta {
    /// Template identifier used in CLI commands (e.g., "constitution")
    pub id: &'static str,
    /// Human-readable name
    pub name: &'static str,
    /// Short description
    pub description: &'static str,
    /// Default output path (relative to project root), may contain `{{var}}` placeholders
    pub default_output: &'static str,
    /// Category: "spec", "harness", or "golden"
    pub category: &'static str,
    /// List of variable names this template expects
    pub variables: &'static [&'static str],
}

/// All available template metadata entries.
pub const TEMPLATES: &[TemplateMeta] = &[
    TemplateMeta {
        id: "constitution",
        name: "Project Constitution",
        description: "Immutable governing principles — rules no agent may violate",
        default_output: "docs/specs/constitution.md",
        category: "spec",
        variables: &["project_name"],
    },
    TemplateMeta {
        id: "spec",
        name: "Feature Specification",
        description: "SDD feature spec with user stories, requirements, and API surface",
        default_output: "docs/specs/spec-{{feature}}.md",
        category: "spec",
        variables: &["feature", "id", "actor", "action", "benefit"],
    },
    TemplateMeta {
        id: "data-model",
        name: "Data Model & Contracts",
        description: "Entity definitions, invariants, database schema, and API contracts",
        default_output: "docs/specs/data-model.md",
        category: "spec",
        variables: &["project_name"],
    },
    TemplateMeta {
        id: "harness",
        name: "Harness Specification",
        description: "HDD test harness with LATS-style trajectories and failure modes",
        default_output: "docs/harness/harness-{{feature}}.md",
        category: "harness",
        variables: &["feature", "id"],
    },
    TemplateMeta {
        id: "playbook",
        name: "Simulation Playbook",
        description: "Multi-agent simulation scenario with CRDT merge verification",
        default_output: "docs/harness/simulation-playbook.md",
        category: "harness",
        variables: &["scenario"],
    },
    TemplateMeta {
        id: "golden",
        name: "Golden Set Evaluation",
        description: "TOML-based evaluation cases with must_contain/must_not_contain checks",
        default_output: ".hief/golden/{{name}}.toml",
        category: "golden",
        variables: &["name", "description"],
    },
];

// ---------------------------------------------------------------------------
// Embedded template content
// ---------------------------------------------------------------------------

pub const CONSTITUTION_TEMPLATE: &str = include_str!("../../templates/specs/constitution.md");
pub const SPEC_TEMPLATE: &str = include_str!("../../templates/specs/spec.md");
pub const DATA_MODEL_TEMPLATE: &str = include_str!("../../templates/specs/data-model.md");
pub const HARNESS_TEMPLATE: &str = include_str!("../../templates/harness/harness.md");
pub const PLAYBOOK_TEMPLATE: &str = include_str!("../../templates/harness/playbook.md");
pub const GOLDEN_TEMPLATE: &str = include_str!("../../templates/golden/golden.toml");

/// Look up a template's content by its ID.
pub fn get_template_content(id: &str) -> Option<&'static str> {
    match id {
        "constitution" => Some(CONSTITUTION_TEMPLATE),
        "spec" => Some(SPEC_TEMPLATE),
        "data-model" => Some(DATA_MODEL_TEMPLATE),
        "harness" => Some(HARNESS_TEMPLATE),
        "playbook" => Some(PLAYBOOK_TEMPLATE),
        "golden" => Some(GOLDEN_TEMPLATE),
        _ => None,
    }
}

/// Look up template metadata by its ID.
pub fn get_template_meta(id: &str) -> Option<&'static TemplateMeta> {
    TEMPLATES.iter().find(|t| t.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_templates_have_content() {
        for meta in TEMPLATES {
            assert!(
                get_template_content(meta.id).is_some(),
                "Template '{}' has metadata but no content",
                meta.id
            );
        }
    }

    #[test]
    fn test_template_meta_lookup() {
        assert!(get_template_meta("constitution").is_some());
        assert!(get_template_meta("spec").is_some());
        assert!(get_template_meta("data-model").is_some());
        assert!(get_template_meta("harness").is_some());
        assert!(get_template_meta("playbook").is_some());
        assert!(get_template_meta("golden").is_some());
        assert!(get_template_meta("nonexistent").is_none());
    }

    #[test]
    fn test_template_content_contains_variables() {
        let content = get_template_content("constitution").expect("failed to load constitution template");
        assert!(content.contains("{{project_name}}"));
    }

    #[test]
    fn test_template_count_matches() {
        assert_eq!(TEMPLATES.len(), 6);
    }
}
