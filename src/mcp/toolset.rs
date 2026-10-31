//! Which MCP tools `hief serve` exposes.
//!
//! Every exposed tool's schema goes into the agent's context on each session,
//! so `[mcp] tools` in hief.toml narrows the list:
//!
//! ```toml
//! [mcp]
//! tools = "minimal"                     # a preset: minimal | default | all
//! tools = ["minimal", "run_evaluation"] # presets and tool names mix
//! ```
//!
//! An unknown name is a startup error, never silently ignored.

use std::collections::BTreeSet;

use crate::errors::{HiefError, Result};

/// Preset names usable in `[mcp] tools`.
pub const PRESETS: &[&str] = &["minimal", "default", "all"];

/// Search, context, drift check, and claim/release.
pub const MINIMAL: &[&str] = &[
    "search_code",
    "structural_search",
    "semantic_search",
    "index_status",
    "get_project_context",
    "get_conventions",
    "get_session_context",
    "check_drift",
    "claim_issue",
    "release_issue",
    "list_claims",
];

/// Tools left out of the `default` preset. They are rarely needed in a
/// session, and `hief skills export` turns the workflows behind them into
/// skill files that cost context only when loaded.
pub const NICHE: &[&str] = &[
    "judge_with_local_model",
    "recover_stale_intents",
    "get_transitive_deps",
    "find_callers",
    "write_context_file",
    "create_pattern",
    "reload_skills",
    "get_routing_table",
    "list_skills",
    "get_skill",
    "get_session_summary",
    "list_context_files",
    "read_context_file",
];

/// Resolve `[mcp] tools` against the tools that exist.
///
/// `available` is every static tool name. `skill_tools` are the dynamic
/// `execute_skill_*` tools: exposed by the `all` and `default` presets, or by
/// naming them. Returns the set of tool names to expose.
pub fn resolve(
    entries: &[String],
    available: &[String],
    skill_tools: &[String],
) -> Result<BTreeSet<String>> {
    let mut out = BTreeSet::new();
    let all: BTreeSet<&String> = available.iter().chain(skill_tools).collect();
    for entry in entries {
        match entry.as_str() {
            "all" => out.extend(all.iter().map(|s| (*s).clone())),
            "default" => {
                out.extend(
                    all.iter()
                        .filter(|s| !NICHE.contains(&s.as_str()))
                        .map(|s| (*s).clone()),
                );
            }
            "minimal" => out.extend(MINIMAL.iter().map(|s| (*s).to_string())),
            name if all.contains(&name.to_string()) => {
                out.insert(name.to_string());
            }
            unknown => {
                return Err(HiefError::Config(format!(
                    "[mcp] tools: unknown tool or preset '{unknown}' (presets: {}; run `hief mcp status` to list tools)",
                    PRESETS.join(", ")
                )));
            }
        }
    }
    if out.is_empty() {
        return Err(HiefError::Config(
            "[mcp] tools: the list is empty, so the server would expose nothing".to_string(),
        ));
    }
    Ok(out)
}

/// Rough token cost of a tool list: JSON size divided by four.
pub fn estimate_tokens(tools: &[rmcp::model::Tool]) -> usize {
    serde_json::to_string(tools).map(|s| s.len()).unwrap_or(0) / 4
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn presets_expand() {
        let all = names(&["search_code", "claim_issue", "find_callers", "check_drift"]);
        let skills = names(&["execute_skill_x"]);
        let r = |e: &[&str]| resolve(&names(e), &all, &skills).unwrap();
        assert_eq!(r(&["all"]).len(), 5);
        let default = r(&["default"]);
        assert!(default.contains("execute_skill_x") && !default.contains("find_callers"));
        assert_eq!(
            r(&["minimal"]),
            ["check_drift", "claim_issue", "search_code"]
                .iter()
                .map(|s| s.to_string())
                .chain(
                    MINIMAL
                        .iter()
                        .filter(|m| !["check_drift", "claim_issue", "search_code"].contains(m))
                        .map(|s| s.to_string())
                )
                .collect()
        );
    }

    #[test]
    fn names_and_presets_mix() {
        let all = names(&["a", "b", "c"]);
        let got = resolve(&names(&["a", "c"]), &all, &[]).unwrap();
        assert_eq!(got.into_iter().collect::<Vec<_>>(), vec!["a", "c"]);
    }

    #[test]
    fn unknown_names_and_empty_lists_are_errors() {
        let all = names(&["a"]);
        let err = resolve(&names(&["nope"]), &all, &[])
            .unwrap_err()
            .to_string();
        assert!(err.contains("unknown tool or preset 'nope'"), "{err}");
        assert!(resolve(&[], &all, &[]).is_err());
    }
}
