//! Document scaffolding and template engine for SDD/HDD artifacts.
//!
//! Provides:
//! - MiniJinja-powered template rendering (`{{ var_name }}` Jinja2 syntax)
//! - Embedded default templates with file-based override support
//! - Auto-population of variables from config, git, and codebase index
//! - Directory scaffolding for docs structure

pub mod templates;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use minijinja::{Environment, Value};
use serde::Serialize;

use crate::config::DocsConfig;
use crate::errors::{HiefError, Result};

// ---------------------------------------------------------------------------
// Template engine (powered by MiniJinja)
// ---------------------------------------------------------------------------

/// Renders a template string with variable substitution using MiniJinja.
///
/// Variables present in `variables` are substituted; unresolved variables
/// remain as `{{variable_name}}` placeholders for the user/agent to fill in.
///
/// Falls back to simple string replacement if MiniJinja encounters a
/// parsing error (e.g. malformed Jinja2 syntax in a template).
pub fn render_template(template: &str, variables: &HashMap<String, String>) -> String {
    match render_with_minijinja(template, variables) {
        Ok(rendered) => rendered,
        Err(e) => {
            tracing::warn!("MiniJinja render failed, using fallback: {}", e);
            render_fallback(template, variables)
        }
    }
}

/// Primary render path using MiniJinja.
///
/// 1. Compiles the template source.
/// 2. Discovers all referenced variables via `undeclared_variables()`.
/// 3. Builds a context where provided variables get their values and
///    unresolved variables get `"{{var_name}}"` as their value (preserving
///    the placeholder in the output).
/// 4. Renders the template.
fn render_with_minijinja(
    template: &str,
    variables: &HashMap<String, String>,
) -> std::result::Result<String, minijinja::Error> {
    let env = Environment::new();
    let tmpl = env.template_from_str(template)?;

    // Discover every variable referenced in the template AST
    let all_vars = tmpl.undeclared_variables(true);

    // Build context: user-supplied values + literal placeholder strings
    // for anything the user didn't provide.
    let mut context: HashMap<String, String> = HashMap::new();
    for var_name in &all_vars {
        if let Some(value) = variables.get(var_name.as_str()) {
            context.insert(var_name.clone(), value.clone());
        } else {
            // Inject the placeholder string literally so it shows up in
            // the rendered output for the user to fill in later.
            context.insert(var_name.clone(), format!("{{{{{}}}}}", var_name));
        }
    }

    let rendered = tmpl.render(Value::from_serialize(&context))?;
    Ok(rendered)
}

/// Fallback renderer: simple `{{key}}` string replacement (no Jinja2
/// features). Used when MiniJinja cannot parse the template source.
fn render_fallback(template: &str, variables: &HashMap<String, String>) -> String {
    let mut result = template.to_string();
    for (key, value) in variables {
        let placeholder = format!("{{{{{}}}}}", key);
        result = result.replace(&placeholder, value);
    }
    result
}

/// Extracts all variable names referenced in a template.
///
/// Uses MiniJinja's AST analysis (`undeclared_variables`) for accurate
/// parsing of Jinja2 syntax, falling back to a simple regex-like scan
/// on parse error.
pub fn extract_variables(template: &str) -> Vec<String> {
    match extract_variables_jinja(template) {
        Some(vars) => vars,
        None => extract_variables_fallback(template),
    }
}

/// Primary extraction path: compile with MiniJinja and query the AST.
fn extract_variables_jinja(template: &str) -> Option<Vec<String>> {
    let env = Environment::new();
    let tmpl = env.template_from_str(template).ok()?;
    let vars = tmpl.undeclared_variables(true);
    let mut sorted: Vec<String> = vars.into_iter().collect();
    sorted.sort();
    Some(sorted)
}

/// Fallback extraction: scan for `{{var_name}}` patterns manually.
fn extract_variables_fallback(template: &str) -> Vec<String> {
    let mut vars = Vec::new();
    let mut rest = template;
    while let Some(start) = rest.find("{{") {
        if let Some(end) = rest[start + 2..].find("}}") {
            let var_name = rest[start + 2..start + 2 + end].trim();
            if !var_name.is_empty() && var_name.chars().all(|c| c.is_alphanumeric() || c == '_') {
                if !vars.contains(&var_name.to_string()) {
                    vars.push(var_name.to_string());
                }
            }
            rest = &rest[start + 2 + end + 2..];
        } else {
            break;
        }
    }
    vars
}

/// Count how many unresolved `{{variable}}` placeholders remain in rendered output.
///
/// Scans the rendered string for `{{...}}` patterns (the literal placeholders
/// that were preserved during rendering for unresolved variables).
pub fn count_unresolved(rendered: &str) -> usize {
    extract_variables_fallback(rendered).len()
}

/// Normalizes generated document content for stable output across environments.
///
/// - Converts CRLF/CR line endings to LF
/// - Ensures the document ends with a single trailing newline
pub fn normalize_generated_document(content: &str) -> String {
    let normalized = content.replace("\r\n", "\n").replace('\r', "\n");
    if normalized.ends_with('\n') {
        normalized
    } else {
        format!("{}\n", normalized)
    }
}

/// Validates whether an SDD LLM prompt includes required HIEF workflow anchors.
///
/// Returns human-readable issue messages. Empty means compliant.
pub fn validate_llm_prompt_contract(prompt: &str) -> Vec<String> {
    let required_anchors = [
        "hief docs generate",
        "docs/specs/constitution.md",
        "{{placeholder}}",
        "search_code",
        "Provide Result",
    ];

    let mut issues = Vec::new();
    for anchor in required_anchors {
        if !prompt.contains(anchor) {
            issues.push(format!(
                "missing required workflow anchor '{}' in SDD_LLM_PROMPT.md",
                anchor
            ));
        }
    }

    issues
}

// ---------------------------------------------------------------------------
// Template resolution (embedded vs file-based)
// ---------------------------------------------------------------------------

/// Resolves a template's content: checks for a file-based override in
/// `.hief/templates/{id}.md` (or `.toml` for golden), falling back to the
/// embedded default.
pub fn resolve_template(project_root: &Path, template_id: &str) -> Result<String> {
    let override_dir = project_root.join(".hief").join("templates");

    // Determine file extension based on template type
    let ext = if template_id == "golden" {
        "toml"
    } else {
        "md"
    };

    let override_path = override_dir.join(format!("{}.{}", template_id, ext));

    if override_path.exists() {
        let content = std::fs::read_to_string(&override_path).map_err(|e| {
            HiefError::Other(format!(
                "failed to read template override {}: {}",
                override_path.display(),
                e
            ))
        })?;
        tracing::debug!(
            "Using file-based template override: {}",
            override_path.display()
        );
        return Ok(content);
    }

    // Fall back to embedded template
    templates::get_template_content(template_id)
        .map(|s| s.to_string())
        .ok_or_else(|| {
            HiefError::Other(format!(
                "unknown template '{}'. Available: {}",
                template_id,
                templates::TEMPLATES
                    .iter()
                    .map(|t| t.id)
                    .collect::<Vec<_>>()
                    .join(", ")
            ))
        })
}

// ---------------------------------------------------------------------------
// Auto-populate variables from project context
// ---------------------------------------------------------------------------

/// Auto-populates variables from available project context:
/// 1. Project name from hief.toml, Cargo.toml, or git remote
/// 2. Index statistics if database is available
pub fn auto_populate_variables(
    project_root: &Path,
    user_vars: &HashMap<String, String>,
) -> HashMap<String, String> {
    let mut vars = user_vars.clone();

    // Auto-populate project_name if not provided
    if !vars.contains_key("project_name") {
        if let Some(name) = detect_project_name(project_root) {
            vars.insert("project_name".to_string(), name);
        }
    }

    vars
}

/// Try to detect the project name from various sources.
fn detect_project_name(project_root: &Path) -> Option<String> {
    // 1. Try Cargo.toml
    let cargo_toml = project_root.join("Cargo.toml");
    if cargo_toml.exists() {
        if let Ok(content) = std::fs::read_to_string(&cargo_toml) {
            if let Ok(parsed) = content.parse::<toml::Table>() {
                if let Some(package) = parsed.get("package").and_then(|p| p.as_table()) {
                    if let Some(name) = package.get("name").and_then(|n| n.as_str()) {
                        return Some(name.to_string());
                    }
                }
            }
        }
    }

    // 2. Try package.json
    let package_json = project_root.join("package.json");
    if package_json.exists() {
        if let Ok(content) = std::fs::read_to_string(&package_json) {
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&content) {
                if let Some(name) = parsed.get("name").and_then(|n| n.as_str()) {
                    return Some(name.to_string());
                }
            }
        }
    }

    // 3. Try git remote
    if let Ok(output) = std::process::Command::new("git")
        .args(["remote", "get-url", "origin"])
        .current_dir(project_root)
        .output()
    {
        if output.status.success() {
            let url = String::from_utf8_lossy(&output.stdout).trim().to_string();
            // Extract repo name from URL: https://github.com/user/repo.git → repo
            if let Some(name) = url
                .rsplit('/')
                .next()
                .map(|s| s.trim_end_matches(".git").to_string())
            {
                if !name.is_empty() {
                    return Some(name);
                }
            }
        }
    }

    // 4. Fall back to directory name
    project_root
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
}

/// Collect index statistics and inject them as template variables.
/// This is used by `hief docs generate` when `--auto-populate` is enabled.
pub async fn auto_populate_from_index(
    db: &crate::db::Database,
    project_root: &Path,
    vars: &mut HashMap<String, String>,
) -> Result<()> {
    let stats = crate::index::status(db, project_root).await?;

    vars.entry("file_count".to_string())
        .or_insert_with(|| stats.total_files.to_string());
    vars.entry("chunk_count".to_string())
        .or_insert_with(|| stats.total_chunks.to_string());

    let lang_list: String = stats
        .languages
        .iter()
        .map(|(lang, count)| format!("{} ({} files)", lang, count))
        .collect::<Vec<_>>()
        .join(", ");
    vars.entry("languages".to_string()).or_insert(lang_list);

    Ok(())
}

// ---------------------------------------------------------------------------
// Output path resolution
// ---------------------------------------------------------------------------

/// Resolves the output path for a generated document.
///
/// Priority:
/// 1. Explicit `--output` flag from CLI
/// 2. Template's default output path (with variable substitution)
/// 3. Computed from config's docs paths
pub fn resolve_output_path(
    project_root: &Path,
    config: &DocsConfig,
    template_id: &str,
    variables: &HashMap<String, String>,
    explicit_output: Option<&str>,
) -> PathBuf {
    if let Some(output) = explicit_output {
        return project_root.join(output);
    }

    if let Some(meta) = templates::get_template_meta(template_id) {
        // Substitute variables in the default output path
        let mut path_str = meta.default_output.to_string();
        for (key, value) in variables {
            let placeholder = format!("{{{{{}}}}}", key);
            path_str = path_str.replace(&placeholder, value);
        }

        // Replace category-level path prefixes with config values
        path_str = path_str.replace("docs/specs/", &format!("{}/", config.specs_path));
        path_str = path_str.replace("docs/harness/", &format!("{}/", config.harness_path));
        // Golden output path is resolved in CLI using eval.golden_set_path.

        return project_root.join(path_str);
    }

    // Fallback
    project_root
        .join(&config.docs_path)
        .join(format!("{}.md", template_id))
}

// ---------------------------------------------------------------------------
// Directory scaffolding
// ---------------------------------------------------------------------------

/// Result of a `docs init` operation.
#[derive(Debug, Serialize)]
pub struct DocsInitReport {
    pub directories_created: Vec<String>,
    pub files_created: Vec<String>,
    pub files_updated: Vec<String>,
    pub already_existed: Vec<String>,
    pub templates_dir: String,
    pub prompt_created: bool,
}

/// Scaffold the default docs directory structure.
pub fn scaffold_docs_dirs(project_root: &Path, config: &DocsConfig) -> Result<DocsInitReport> {
    let mut report = DocsInitReport {
        directories_created: Vec::new(),
        files_created: Vec::new(),
        files_updated: Vec::new(),
        already_existed: Vec::new(),
        templates_dir: String::new(),
        prompt_created: false,
    };

    // Directories to create
    let dirs = [&config.specs_path, &config.harness_path];

    for dir in &dirs {
        let full_path = project_root.join(dir);
        if full_path.exists() {
            report.already_existed.push(dir.to_string());
        } else {
            std::fs::create_dir_all(&full_path)?;
            report.directories_created.push(dir.to_string());
        }

        // Add .gitkeep if directory is empty
        let gitkeep = full_path.join(".gitkeep");
        if !gitkeep.exists() && is_dir_empty(&full_path) {
            std::fs::write(&gitkeep, "")?;
        }
    }

    // Create .hief/templates/ for user overrides
    let templates_dir = project_root.join(".hief").join("templates");
    if templates_dir.exists() {
        report.already_existed.push(".hief/templates/".to_string());
    } else {
        std::fs::create_dir_all(&templates_dir)?;
        report
            .directories_created
            .push(".hief/templates/".to_string());

        // Write a README explaining template overrides
        let readme = templates_dir.join("README.md");
        std::fs::write(&readme, TEMPLATES_README)?;
        report
            .files_created
            .push(".hief/templates/README.md".to_string());
    }

    // Always generate/update the LLM Prompt so users get the latest version
    let prompt_file = templates_dir.join("SDD_LLM_PROMPT.md");
    let prompt_existed_before = prompt_file.exists();
    std::fs::write(&prompt_file, LLM_PROMPT_TEMPLATE)?;
    if !prompt_existed_before {
        report
            .files_created
            .push(".hief/templates/SDD_LLM_PROMPT.md".to_string());
    } else {
        report
            .files_updated
            .push(".hief/templates/SDD_LLM_PROMPT.md".to_string());
    }
    report.prompt_created = true;

    // Generate framework-specific templates
    let frameworks_dir = templates_dir.join("frameworks");
    if !frameworks_dir.exists() {
        std::fs::create_dir_all(&frameworks_dir)?;
        report
            .directories_created
            .push(".hief/templates/frameworks/".to_string());
    }

    let framework_templates = [
        ("rust.md", RUST_FRAMEWORK_TEMPLATE),
        ("python.md", PYTHON_FRAMEWORK_TEMPLATE),
        ("tauri.md", TAURI_FRAMEWORK_TEMPLATE),
        ("typescript.md", TYPESCRIPT_FRAMEWORK_TEMPLATE),
        ("java.md", JAVA_FRAMEWORK_TEMPLATE),
        ("golang.md", GOLANG_FRAMEWORK_TEMPLATE),
    ];

    for (filename, content) in framework_templates {
        let file_path = frameworks_dir.join(filename);
        if !file_path.exists() {
            std::fs::write(&file_path, content)?;
            report
                .files_created
                .push(format!(".hief/templates/frameworks/{}", filename));
        }
    }

    sync_llm_instruction_files(project_root, &mut report)?;

    report.templates_dir = templates_dir.display().to_string();

    Ok(report)
}

fn is_dir_empty(path: &Path) -> bool {
    path.read_dir()
        .map(|mut entries| entries.next().is_none())
        .unwrap_or(true)
}

fn sync_llm_instruction_files(project_root: &Path, report: &mut DocsInitReport) -> Result<()> {
    let candidates = ["AGENTS.md", "GEMINI.md", "CLAUDE.md"];

    let mut found_any = false;
    for name in candidates {
        let path = project_root.join(name);
        if !path.exists() {
            continue;
        }

        found_any = true;
        let existing = std::fs::read_to_string(&path).unwrap_or_default();
        let updated = upsert_managed_block(
            &existing,
            HIEF_WORKFLOW_BLOCK_START,
            HIEF_WORKFLOW_BLOCK_END,
            HIEF_WORKFLOW_BLOCK,
        );

        if updated != existing {
            std::fs::write(&path, updated)?;
            report.files_updated.push(name.to_string());
        } else {
            report.already_existed.push(name.to_string());
        }
    }

    if !found_any {
        let fallback = project_root.join("AGENTS.md");
        let content = format!(
            "# AGENTS.md\n\n{}\n",
            HIEF_WORKFLOW_BLOCK.trim_end()
        );
        std::fs::write(&fallback, content)?;
        report.files_created.push("AGENTS.md".to_string());
    }

    Ok(())
}

fn upsert_managed_block(
    content: &str,
    start_marker: &str,
    end_marker: &str,
    block_content: &str,
) -> String {
    if let Some(start) = content.find(start_marker) {
        if let Some(end_rel) = content[start..].find(end_marker) {
            let end = start + end_rel + end_marker.len();
            let mut result = String::new();
            result.push_str(&content[..start]);
            if !result.ends_with('\n') {
                result.push('\n');
            }
            result.push_str(block_content);
            if !block_content.ends_with('\n') {
                result.push('\n');
            }
            let tail = content[end..].trim_start_matches('\n');
            if !tail.is_empty() {
                result.push('\n');
                result.push_str(tail);
            }
            return normalize_generated_document(&result);
        }
    }

    let mut result = content.to_string();
    if !result.is_empty() && !result.ends_with('\n') {
        result.push('\n');
    }
    if !result.is_empty() {
        result.push('\n');
    }
    result.push_str(block_content);
    normalize_generated_document(&result)
}

// ---------------------------------------------------------------------------
// Docs structure check
// ---------------------------------------------------------------------------

/// Result of a `docs check` operation.
#[derive(Debug, Serialize)]
pub struct DocsCheckReport {
    pub healthy: bool,
    pub checks: Vec<DocsCheckItem>,
}

#[derive(Debug, Serialize)]
pub struct DocsCheckItem {
    pub name: String,
    pub status: String, // "ok", "missing", "warning"
    pub message: String,
}

/// Check the docs directory structure and report on completeness.
pub fn check_docs_structure(project_root: &Path, config: &DocsConfig) -> DocsCheckReport {
    let mut checks = Vec::new();

    // Check docs directories
    for (name, path) in [
        ("specs_dir", &config.specs_path),
        ("harness_dir", &config.harness_path),
    ] {
        let full_path = project_root.join(path);
        if full_path.exists() {
            let file_count = count_files_in_dir(&full_path);
            checks.push(DocsCheckItem {
                name: name.to_string(),
                status: "ok".to_string(),
                message: format!("{} exists ({} files)", path, file_count),
            });
        } else {
            checks.push(DocsCheckItem {
                name: name.to_string(),
                status: "missing".to_string(),
                message: format!("{} not found — run `hief docs init`", path),
            });
        }
    }

    // Check for key documents
    let key_docs = [
        (
            "constitution",
            config.specs_path.clone() + "/constitution.md",
        ),
        ("data-model", config.specs_path.clone() + "/data-model.md"),
    ];

    for (name, path) in &key_docs {
        let full_path = project_root.join(path);
        if full_path.exists() {
            checks.push(DocsCheckItem {
                name: name.to_string(),
                status: "ok".to_string(),
                message: format!("{} exists", path),
            });
        } else {
            checks.push(DocsCheckItem {
                name: name.to_string(),
                status: "warning".to_string(),
                message: format!("{} not found — run `hief docs generate {}`", path, name),
            });
        }
    }

    // Check for template overrides
    let templates_dir = project_root.join(".hief").join("templates");
    if templates_dir.exists() {
        let override_count = count_files_in_dir(&templates_dir);
        // Subtract README.md from count
        let actual_overrides = if override_count > 0 {
            override_count - 1
        } else {
            0
        };
        checks.push(DocsCheckItem {
            name: "template_overrides".to_string(),
            status: "ok".to_string(),
            message: format!(
                ".hief/templates/ exists ({} custom template{})",
                actual_overrides,
                if actual_overrides == 1 { "" } else { "s" }
            ),
        });
    } else {
        checks.push(DocsCheckItem {
            name: "template_overrides".to_string(),
            status: "ok".to_string(),
            message: ".hief/templates/ not found (using embedded defaults)".to_string(),
        });
    }

    // Check that SDD LLM prompt exists and carries required framework contract anchors
    let prompt_file = project_root
        .join(".hief")
        .join("templates")
        .join("SDD_LLM_PROMPT.md");
    if !prompt_file.exists() {
        checks.push(DocsCheckItem {
            name: "llm_prompt_contract".to_string(),
            status: "warning".to_string(),
            message:
                "Missing .hief/templates/SDD_LLM_PROMPT.md — run `hief docs init` to enforce LLM workflow"
                    .to_string(),
        });
    } else {
        match std::fs::read_to_string(&prompt_file) {
            Ok(content) => {
                let issues = validate_llm_prompt_contract(&content);
                if issues.is_empty() {
                    checks.push(DocsCheckItem {
                        name: "llm_prompt_contract".to_string(),
                        status: "ok".to_string(),
                        message: "SDD_LLM_PROMPT.md includes required HIEF workflow anchors"
                            .to_string(),
                    });
                } else {
                    checks.push(DocsCheckItem {
                        name: "llm_prompt_contract".to_string(),
                        status: "warning".to_string(),
                        message: format!(
                            "SDD_LLM_PROMPT.md is missing {} anchor(s): {}",
                            issues.len(),
                            issues.join("; ")
                        ),
                    });
                }
            }
            Err(e) => {
                checks.push(DocsCheckItem {
                    name: "llm_prompt_contract".to_string(),
                    status: "warning".to_string(),
                    message: format!("Failed to read SDD_LLM_PROMPT.md: {}", e),
                });
            }
        }
    }

    // Check for any feature specs
    let specs_dir = project_root.join(&config.specs_path);
    if specs_dir.exists() {
        let spec_count = count_matching_files(&specs_dir, "spec-");
        if spec_count == 0 {
            checks.push(DocsCheckItem {
                name: "feature_specs".to_string(),
                status: "warning".to_string(),
                message: "No feature specs found — run `hief docs generate spec --name <feature>`"
                    .to_string(),
            });
        } else {
            checks.push(DocsCheckItem {
                name: "feature_specs".to_string(),
                status: "ok".to_string(),
                message: format!(
                    "{} feature spec{} found",
                    spec_count,
                    if spec_count == 1 { "" } else { "s" }
                ),
            });
        }

        // Consistency check: report unresolved placeholders in specs docs
        let unresolved_in_specs = count_unresolved_placeholders_in_dir(&specs_dir);
        if unresolved_in_specs > 0 {
            checks.push(DocsCheckItem {
                name: "spec_placeholders".to_string(),
                status: "warning".to_string(),
                message: format!(
                    "{} unresolved placeholder(s) found across {} — fill template variables before review",
                    unresolved_in_specs,
                    config.specs_path
                ),
            });
        } else {
            checks.push(DocsCheckItem {
                name: "spec_placeholders".to_string(),
                status: "ok".to_string(),
                message: format!(
                    "No unresolved placeholders found in {}",
                    config.specs_path
                ),
            });
        }
    }

    // Warn about stray golden files in specs dir (the strategic example golden
    // may live under docs/specs; this check nudges authors to relocate it to the
    // standard `.hief/golden/` directory).
    if specs_dir.exists() {
        let stray = specs_dir
            .read_dir()
            .into_iter()
            .flatten()
            .filter(|e| {
                if let Ok(entry) = e {
                    return entry
                        .path()
                        .extension()
                        .and_then(|s| s.to_str())
                        .map(|ext| ext == "toml")
                        .unwrap_or(false);
                }
                false
            })
            .count();
        if stray > 0 {
            checks.push(DocsCheckItem {
                name: "stray_golden".to_string(),
                status: "warning".to_string(),
                message: format!(
                    "Found {} TOML file{} under {} — golden sets belong in .hief/golden/",
                    stray,
                    if stray == 1 { "" } else { "s" },
                    config.specs_path
                ),
            });
        }
    }

    // Check for harness specs
    let harness_dir = project_root.join(&config.harness_path);
    if harness_dir.exists() {
        let harness_count = count_matching_files(&harness_dir, "harness-");
        if harness_count == 0 {
            checks.push(DocsCheckItem {
                name: "harness_specs".to_string(),
                status: "warning".to_string(),
                message:
                    "No harness specs found — run `hief docs generate harness --name <feature>`"
                        .to_string(),
            });
        } else {
            checks.push(DocsCheckItem {
                name: "harness_specs".to_string(),
                status: "ok".to_string(),
                message: format!(
                    "{} harness spec{} found",
                    harness_count,
                    if harness_count == 1 { "" } else { "s" }
                ),
            });
        }

        // Consistency check: report unresolved placeholders in harness docs
        let unresolved_in_harness = count_unresolved_placeholders_in_dir(&harness_dir);
        if unresolved_in_harness > 0 {
            checks.push(DocsCheckItem {
                name: "harness_placeholders".to_string(),
                status: "warning".to_string(),
                message: format!(
                    "{} unresolved placeholder(s) found across {} — fill template variables before review",
                    unresolved_in_harness,
                    config.harness_path
                ),
            });
        } else {
            checks.push(DocsCheckItem {
                name: "harness_placeholders".to_string(),
                status: "ok".to_string(),
                message: format!(
                    "No unresolved placeholders found in {}",
                    config.harness_path
                ),
            });
        }
    }

    // AST LINT: Check data-model.md structs against codebase
    let data_model_path = project_root.join(&config.specs_path).join("data-model.md");
    if data_model_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&data_model_path) {
            let mut expected_structs = Vec::new();
            for line in content.lines() {
                if line.starts_with("pub struct ") {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 3 {
                        let name = parts[2].trim_end_matches('{').trim();
                        if !name.is_empty() {
                            expected_structs.push(name.to_string());
                        }
                    }
                }
            }

            for root_struct in expected_structs {
                let pattern = format!("pub struct {} $$$", root_struct);
                let query = crate::index::structural::StructuralQuery {
                    pattern,
                    language: "rust".to_string(),
                    top_k: 1,
                };

                match crate::index::structural::search(project_root, &query) {
                    Ok(matches) => {
                        if matches.is_empty() {
                            checks.push(DocsCheckItem {
                                name: format!("data_model_sync_{}", root_struct),
                                status: "warning".to_string(),
                                message: format!("Struct '{}' from data-model.md not found in codebase (AST lint)", root_struct),
                            });
                        } else {
                            checks.push(DocsCheckItem {
                                name: format!("data_model_sync_{}", root_struct),
                                status: "ok".to_string(),
                                message: format!("Struct '{}' aligns with codebase", root_struct),
                            });
                        }
                    }
                    Err(_) => {
                        checks.push(DocsCheckItem {
                            name: format!("data_model_sync_{}", root_struct),
                            status: "warning".to_string(),
                            message: format!("Failed to run AST lint for struct '{}'", root_struct),
                        });
                    }
                }
            }
        }
    }

    let healthy = checks.iter().all(|c| c.status != "missing");

    DocsCheckReport { healthy, checks }
}

fn count_files_in_dir(path: &Path) -> usize {
    path.read_dir()
        .map(|entries| {
            entries
                .filter_map(|e| e.ok())
                .filter(|e| e.file_type().map(|ft| ft.is_file()).unwrap_or(false))
                .count()
        })
        .unwrap_or(0)
}

fn count_matching_files(dir: &Path, prefix: &str) -> usize {
    dir.read_dir()
        .map(|entries| {
            entries
                .filter_map(|e| e.ok())
                .filter(|e| e.file_name().to_string_lossy().starts_with(prefix))
                .count()
        })
        .unwrap_or(0)
}

fn count_unresolved_placeholders_in_dir(dir: &Path) -> usize {
    dir.read_dir()
        .map(|entries| {
            entries
                .filter_map(|e| e.ok())
                .filter(|e| e.file_type().map(|ft| ft.is_file()).unwrap_or(false))
                .filter(|e| {
                    e.path()
                        .extension()
                        .and_then(|ext| ext.to_str())
                        .map(|ext| ext == "md")
                        .unwrap_or(false)
                })
                .map(|e| {
                    std::fs::read_to_string(e.path())
                        .map(|content| count_unresolved(&content))
                        .unwrap_or(0)
                })
                .sum()
        })
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const TEMPLATES_README: &str = r#"# Custom Template Overrides

Place template files here to override HIEF's embedded defaults.
Templates use `{{variable_name}}` syntax for variable substitution.

## Supported Template Files

| File | Overrides |
|------|-----------|
| `constitution.md` | Project constitution template |
| `spec.md` | Feature specification template |
| `data-model.md` | Data model & contracts template |
| `harness.md` | Harness specification template |
| `playbook.md` | Simulation playbook template |
| `golden.toml` | Golden set evaluation template |

## Variables

Variables are substituted when generating documents. Common variables:

- `{{project_name}}` — Project name (auto-detected from Cargo.toml/package.json/git)
- `{{feature}}` — Feature name (from `--name` flag)
- `{{id}}` — Intent ID (from `--id` flag or auto-generated)
- `{{file_count}}` — Total indexed files (auto-populated from index)
- `{{languages}}` — Indexed languages (auto-populated from index)
- `{{chunk_count}}` — Total indexed chunks (auto-populated from index)

Unresolved variables remain as `{{placeholder}}` for manual editing.

## Example

To customize the constitution template:

```bash
cp $(hief docs show-template constitution) .hief/templates/constitution.md
# Edit .hief/templates/constitution.md
hief docs generate constitution
```
"#;

const LLM_PROMPT_TEMPLATE: &str = r#"# Role
You are an expert software architect and technical writer specializing in Spec-Driven Development (SDD) for HIEF (Hybrid Intent-Evaluation Framework).

# Context
This repository uses HIEF for organizing feature specs, data models, and architectural rules.
Reference templates and writing approaches:
- HIEF templates: https://github.com/hiranp/hief/tree/main/templates
- AWS Kiro prompt library: https://aws.amazon.com/startups/prompt-library/kiro-project-init?lang=en-US
- GitHub Spec-Kit: https://github.com/github/spec-kit?tab=readme-ov-file#-specify-cli-reference
- Tessl quickstart docs/rules: https://docs.tessl.io/introduction-to-tessl/quickstart-skills-docs-rules

# Mandatory Workflow (Do Not Skip)
1. Scaffold first via HIEF CLI:
    - Feature spec: `hief docs generate spec --name <feature>`
    - Data model: `hief docs generate data-model`
    - Harness: `hief docs generate harness --name <feature>`
2. Read and obey `docs/specs/constitution.md` before writing.
3. Ground content in the real codebase using local tools (`search_code`, structural search, and project files).
4. Replace every `{{placeholder}}` with concrete, technically accurate content.
5. Provide Result by editing the generated file directly (preferred) or returning final markdown.

# Enforcement Rules
- Never draft spec files freehand when a matching HIEF template exists.
- If required context is missing, state assumptions explicitly and continue with best-effort draft.
- Before finalizing, ensure no unresolved placeholders remain in the document.

# Quality Bar
- Be concise, specific, and technically precise.
- Prefer verifiable acceptance criteria and explicit invariants.
- Keep API contracts actionable and implementation-aware.
"#;

const HIEF_WORKFLOW_BLOCK_START: &str = "<!-- HIEF:MANDATORY_WORKFLOW:START -->";
const HIEF_WORKFLOW_BLOCK_END: &str = "<!-- HIEF:MANDATORY_WORKFLOW:END -->";
const HIEF_WORKFLOW_BLOCK: &str = r#"<!-- HIEF:MANDATORY_WORKFLOW:START -->
## HIEF Mandatory Workflow

1. Run `hief docs init` to scaffold/update templates and workflow rules.
2. Generate docs via templates only:
    - `hief docs generate spec --name <feature>`
    - `hief docs generate data-model`
    - `hief docs generate harness --name <feature>`
3. Read and obey `docs/specs/constitution.md`.
4. Ground content in local code context (`search_code`, structural search, and source files).
5. Replace all `{{placeholder}}` values before review.

Reference URLs:
- https://github.com/hiranp/hief/tree/main/templates
- https://aws.amazon.com/startups/prompt-library/kiro-project-init?lang=en-US
- https://github.com/github/spec-kit?tab=readme-ov-file#-specify-cli-reference
- https://docs.tessl.io/introduction-to-tessl/quickstart-skills-docs-rules
<!-- HIEF:MANDATORY_WORKFLOW:END -->
"#;

const RUST_FRAMEWORK_TEMPLATE: &str = include_str!("../../templates/frameworks/rust.md");
const PYTHON_FRAMEWORK_TEMPLATE: &str = include_str!("../../templates/frameworks/python.md");
const TAURI_FRAMEWORK_TEMPLATE: &str = include_str!("../../templates/frameworks/tauri.md");
const TYPESCRIPT_FRAMEWORK_TEMPLATE: &str =
    include_str!("../../templates/frameworks/typescript.md");
const JAVA_FRAMEWORK_TEMPLATE: &str = include_str!("../../templates/frameworks/java.md");
const GOLANG_FRAMEWORK_TEMPLATE: &str = include_str!("../../templates/frameworks/golang.md");

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_template_basic() {
        let template = "Hello, {{name}}! Welcome to {{project}}.";
        let mut vars = HashMap::new();
        vars.insert("name".to_string(), "Alice".to_string());
        vars.insert("project".to_string(), "HIEF".to_string());

        let result = render_template(template, &vars);
        assert_eq!(result, "Hello, Alice! Welcome to HIEF.");
    }

    #[test]
    fn test_render_template_unresolved_remain() {
        let template = "{{resolved}} and {{unresolved}}";
        let mut vars = HashMap::new();
        vars.insert("resolved".to_string(), "YES".to_string());

        let result = render_template(template, &vars);
        assert_eq!(result, "YES and {{unresolved}}");
    }

    #[test]
    fn test_render_template_empty_vars() {
        let template = "{{a}} {{b}} {{c}}";
        let vars = HashMap::new();
        let result = render_template(template, &vars);
        assert_eq!(result, "{{a}} {{b}} {{c}}");
    }

    #[test]
    fn test_render_template_repeated_var() {
        let template = "{{x}} is {{x}}, not {{y}}";
        let mut vars = HashMap::new();
        vars.insert("x".to_string(), "foo".to_string());

        let result = render_template(template, &vars);
        assert_eq!(result, "foo is foo, not {{y}}");
    }

    #[test]
    fn test_extract_variables() {
        let template = "Hello {{name}}, your project is {{project_name}}. {{name}} again.";
        let vars = extract_variables(template);
        assert_eq!(vars, vec!["name", "project_name"]);
    }

    #[test]
    fn test_extract_variables_no_vars() {
        let vars = extract_variables("No variables here.");
        assert!(vars.is_empty());
    }

    #[test]
    fn test_extract_variables_ignores_invalid() {
        // Things like {{}} or {{with spaces}} should be ignored
        let template = "{{}} and {{with spaces}} and {{valid_var}}";
        let vars = extract_variables(template);
        assert_eq!(vars, vec!["valid_var"]);
    }

    #[test]
    fn test_count_unresolved() {
        let rendered = "Hello Alice! {{placeholder_1}} and {{placeholder_2}}.";
        assert_eq!(count_unresolved(rendered), 2);
    }

    #[test]
    fn test_count_unresolved_none() {
        assert_eq!(count_unresolved("No placeholders here."), 0);
    }

    #[test]
    fn test_normalize_generated_document_newline_and_lf() {
        let input = "line1\r\nline2\rline3";
        let output = normalize_generated_document(input);
        assert_eq!(output, "line1\nline2\nline3\n");
    }

    #[test]
    fn test_validate_llm_prompt_contract_valid() {
        let prompt = "hief docs generate\ndocs/specs/constitution.md\n{{placeholder}}\nsearch_code\nProvide Result";
        let issues = validate_llm_prompt_contract(prompt);
        assert!(issues.is_empty());
    }

    #[test]
    fn test_validate_llm_prompt_contract_missing() {
        let prompt = "only partial prompt";
        let issues = validate_llm_prompt_contract(prompt);
        assert!(!issues.is_empty());
    }

    #[test]
    fn test_resolve_output_path_explicit() {
        let root = Path::new("/tmp/project");
        let config = DocsConfig::default();
        let vars = HashMap::new();

        let path = resolve_output_path(root, &config, "constitution", &vars, Some("my/output.md"));
        assert_eq!(path, PathBuf::from("/tmp/project/my/output.md"));
    }

    #[test]
    fn test_resolve_output_path_default() {
        let root = Path::new("/tmp/project");
        let config = DocsConfig::default();
        let vars = HashMap::new();

        let path = resolve_output_path(root, &config, "constitution", &vars, None);
        assert_eq!(
            path,
            PathBuf::from("/tmp/project/docs/specs/constitution.md")
        );
    }

    #[test]
    fn test_resolve_output_path_with_variable() {
        let root = Path::new("/tmp/project");
        let config = DocsConfig::default();
        let mut vars = HashMap::new();
        vars.insert("feature".to_string(), "search".to_string());

        let path = resolve_output_path(root, &config, "spec", &vars, None);
        assert_eq!(
            path,
            PathBuf::from("/tmp/project/docs/specs/spec-search.md")
        );
    }

    #[test]
    fn test_scaffold_docs_dirs() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();

        // Create .hief dir (prerequisite)
        std::fs::create_dir_all(root.join(".hief")).unwrap();

        let config = DocsConfig::default();
        let report = scaffold_docs_dirs(root, &config).unwrap();

        assert!(root.join("docs/specs").exists());
        assert!(root.join("docs/harness").exists());
        assert!(root.join(".hief/templates").exists());
        assert!(root.join(".hief/templates/README.md").exists());
        assert!(!report.directories_created.is_empty());
    }

    #[test]
    fn test_scaffold_docs_dirs_idempotent() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        std::fs::create_dir_all(root.join(".hief")).unwrap();

        let config = DocsConfig::default();

        // First call creates
        let report1 = scaffold_docs_dirs(root, &config).unwrap();
        assert!(!report1.directories_created.is_empty());

        // Second call is idempotent
        let report2 = scaffold_docs_dirs(root, &config).unwrap();
        assert!(report2.directories_created.is_empty());
        assert!(!report2.already_existed.is_empty());
    }

    #[test]
    fn test_check_docs_structure_empty() {
        let tmp = tempfile::tempdir().unwrap();
        let config = DocsConfig::default();

        let report = check_docs_structure(tmp.path(), &config);
        assert!(!report.healthy); // dirs are missing
    }

    #[test]
    fn test_check_docs_structure_after_init() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        std::fs::create_dir_all(root.join(".hief")).unwrap();

        let config = DocsConfig::default();
        scaffold_docs_dirs(root, &config).unwrap();

        let report = check_docs_structure(root, &config);
        assert!(report.healthy); // dirs exist now
    }

    // -----------------------------------------------------------------------
    // MiniJinja-specific tests — Jinja2 features beyond simple substitution
    // -----------------------------------------------------------------------

    #[test]
    fn test_minijinja_conditionals() {
        let template = "{% if has_api %}API: {{api_name}}{% else %}No API{% endif %}";
        let mut vars = HashMap::new();
        vars.insert("has_api".to_string(), "true".to_string());
        vars.insert("api_name".to_string(), "search_code".to_string());

        let result = render_template(template, &vars);
        assert_eq!(result, "API: search_code");
    }

    #[test]
    fn test_minijinja_conditionals_false_branch() {
        // MiniJinja treats empty string as falsy
        let template = "{% if has_api %}API: {{api_name}}{% else %}No API{% endif %}";
        let mut vars = HashMap::new();
        vars.insert("has_api".to_string(), "".to_string());

        let result = render_template(template, &vars);
        assert_eq!(result, "No API");
    }

    #[test]
    fn test_minijinja_loops() {
        let template = "Languages: {% for lang in languages %}{{lang}}{% if not loop.last %}, {% endif %}{% endfor %}";
        // MiniJinja needs a list value; pass via render_with_minijinja directly
        let env = Environment::new();
        let tmpl = env.template_from_str(template).unwrap();
        let ctx = minijinja::context! {
            languages => vec!["rust", "python", "typescript"],
        };
        let result = tmpl.render(ctx).unwrap();
        assert_eq!(result, "Languages: rust, python, typescript");
    }

    #[test]
    fn test_minijinja_filters() {
        let template = "Project: {{name|upper}}";
        let mut vars = HashMap::new();
        vars.insert("name".to_string(), "hief".to_string());

        let result = render_template(template, &vars);
        assert_eq!(result, "Project: HIEF");
    }

    #[test]
    fn test_minijinja_default_filter() {
        let template = "Author: {{author|default('unknown')}}";
        let vars: HashMap<String, String> = HashMap::new();

        // MiniJinja with default filter — the placeholder won't be preserved
        // because `default()` explicitly handles undefined
        let env = Environment::new();
        let tmpl = env.template_from_str(template).unwrap();
        let result = tmpl.render(Value::from_serialize(&vars)).unwrap();
        assert_eq!(result, "Author: unknown");
    }

    #[test]
    fn test_minijinja_whitespace_in_braces() {
        // MiniJinja handles both {{ var }} and {{var}} identically
        let template = "Hello {{ name }} and {{project}}!";
        let mut vars = HashMap::new();
        vars.insert("name".to_string(), "Alice".to_string());
        vars.insert("project".to_string(), "HIEF".to_string());

        let result = render_template(template, &vars);
        assert_eq!(result, "Hello Alice and HIEF!");
    }

    #[test]
    fn test_minijinja_renders_embedded_constitution() {
        let content = templates::get_template_content("constitution").unwrap();
        let mut vars = HashMap::new();
        vars.insert("project_name".to_string(), "TestProject".to_string());

        let result = render_template(content, &vars);
        assert!(result.contains("# Project Constitution: TestProject"));
        assert!(result.contains("**Name:** TestProject"));
        // Unresolved vars should be preserved as placeholders
        assert!(result.contains("{{purpose}}"));
        assert!(result.contains("{{architecture}}"));
    }

    #[test]
    fn test_minijinja_renders_embedded_spec() {
        let content = templates::get_template_content("spec").unwrap();
        let mut vars = HashMap::new();
        vars.insert("feature".to_string(), "code-search".to_string());
        vars.insert("id".to_string(), "abc123".to_string());
        vars.insert("actor".to_string(), "developer".to_string());

        let result = render_template(content, &vars);
        assert!(result.contains("# Feature Spec: code-search"));
        assert!(result.contains("**Intent:** hief-abc123"));
        assert!(result.contains("**As a** developer"));
        // Unresolved vars preserved
        assert!(result.contains("{{action}}"));
        assert!(result.contains("{{benefit}}"));
    }

    #[test]
    fn test_minijinja_renders_embedded_golden() {
        let content = templates::get_template_content("golden").unwrap();
        let mut vars = HashMap::new();
        vars.insert("name".to_string(), "quality-check".to_string());
        vars.insert(
            "description".to_string(),
            "Basic quality checks".to_string(),
        );

        let result = render_template(content, &vars);
        assert!(result.contains("name = \"quality-check\""));
        assert!(result.contains("description = \"Basic quality checks\""));
    }

    #[test]
    fn test_fallback_when_minijinja_fails() {
        // Template with malformed Jinja2 syntax should fall back gracefully
        let template = "Hello {{name}}! {% broken tag %}";
        let mut vars = HashMap::new();
        vars.insert("name".to_string(), "Alice".to_string());

        let result = render_template(template, &vars);
        // Fallback renderer does simple string replacement
        assert!(result.contains("Hello Alice!"));
    }

    #[test]
    fn test_extract_variables_from_all_templates() {
        // Ensure extract_variables works on every embedded template
        for meta in templates::TEMPLATES {
            let content = templates::get_template_content(meta.id).unwrap();
            let vars = extract_variables(content);
            assert!(
                !vars.is_empty(),
                "Template '{}' should have at least one variable",
                meta.id
            );
            // Every primary variable listed in meta should appear in the template
            for &expected_var in meta.variables {
                assert!(
                    vars.contains(&expected_var.to_string()),
                    "Template '{}' metadata lists variable '{}' but it wasn't found in template content. Found: {:?}",
                    meta.id,
                    expected_var,
                    vars,
                );
            }
        }
    }
}
