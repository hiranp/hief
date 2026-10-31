//! Four checkers for the ways agent-maintained repos rot:
//! `todo-issue`, `cited-paths`, `reference-freshness`, `instruction-literals`.
//!
//! Each one emits ordinary [`DriftIssue`]s, with `file` set to `path:line`
//! where a line is known, so the baseline and `--diff-base` apply to them.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

use ignore::WalkBuilder;
use ignore::gitignore::{Gitignore, GitignoreBuilder};

use super::{DriftIssue, DriftSeverity};
use crate::config::Config;

pub const TODO_ISSUE: &str = "todo-issue";
pub const CITED_PATHS: &str = "cited-paths";
pub const REFERENCE_FRESHNESS: &str = "reference-freshness";
pub const INSTRUCTION_LITERALS: &str = "instruction-literals";

/// Agent instruction files, relative to the project root.
const INSTRUCTION_FILES: &[&str] = &[
    "AGENTS.md",
    "CLAUDE.md",
    "GEMINI.md",
    ".github/copilot-instructions.md",
];

const TODO_EXTENSIONS: &[&str] = &[
    "rs", "py", "ts", "tsx", "js", "jsx", "go", "java", "kt", "c", "h", "cpp", "hpp", "cs", "rb",
    "sh", "bash", "swift",
];

fn add(
    issues: &mut Vec<DriftIssue>,
    checker: &str,
    file: String,
    message: String,
    severity: DriftSeverity,
) {
    issues.push(DriftIssue {
        checker: checker.to_string(),
        file,
        message,
        severity,
    });
}

fn rel(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

// ---------------------------------------------------------------- markdown

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    /// Text inside single backticks.
    Inline,
    /// A line inside a fenced code block.
    Fence,
}

struct Segment {
    line: usize,
    text: String,
    kind: Kind,
}

/// Split markdown into inline code spans and fenced code lines. Fences whose
/// info string mentions "example" are skipped: they show hypothetical input.
fn md_segments(content: &str) -> Vec<Segment> {
    let mut out = Vec::new();
    let mut fence: Option<bool> = None; // Some(is_example) while inside a fence
    for (i, raw) in content.lines().enumerate() {
        let trimmed = raw.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            fence = match fence {
                Some(_) => None,
                None => Some(trimmed.to_ascii_lowercase().contains("example")),
            };
            continue;
        }
        match fence {
            Some(true) => {}
            Some(false) => out.push(Segment {
                line: i + 1,
                text: raw.trim().to_string(),
                kind: Kind::Fence,
            }),
            None => {
                for (n, part) in raw.split('`').enumerate() {
                    if n % 2 == 1 && !part.trim().is_empty() {
                        out.push(Segment {
                            line: i + 1,
                            text: part.trim().to_string(),
                            kind: Kind::Inline,
                        });
                    }
                }
            }
        }
    }
    out
}

fn markdown_files(root: &Path, include_docs: bool) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = INSTRUCTION_FILES
        .iter()
        .map(|f| root.join(f))
        .filter(|p| p.is_file())
        .collect();
    if include_docs {
        let docs = root.join("docs");
        if docs.is_dir() {
            for entry in WalkBuilder::new(&docs).build().flatten() {
                let p = entry.path();
                if p.extension().is_some_and(|e| e == "md")
                    && p.is_file()
                    && !files.contains(&p.to_path_buf())
                {
                    files.push(p.to_path_buf());
                }
            }
        }
    }
    files.sort();
    files
}

fn has_placeholder(s: &str) -> bool {
    s.contains(['<', '>', '$', '{', '}', '*', '…']) || s.contains("...")
}

// -------------------------------------------------------------- todo-issue

/// Parse one source line: `Some(None)` = marker without an issue ref,
/// `Some(Some(n))` = marker with `(#n)`, `None` = no marker.
///
/// The marker must open a comment (`// TODO`, `# FIXME(#3)`), and the comment
/// token must not sit inside a string literal. That keeps prose and test
/// fixtures that merely mention TODO out of the results.
fn todo_on_line(line: &str) -> Option<Option<u64>> {
    let mut starts: Vec<usize> = ["//", "/*", "<!--", "--", "#"]
        .iter()
        .flat_map(|t| line.match_indices(t).map(|(i, _)| i))
        .collect();
    if line.trim_start().starts_with('*') {
        starts.push(line.len() - line.trim_start().len());
    }
    for at in starts {
        if line[..at].matches('"').count() % 2 == 1 || line[..at].ends_with('`') {
            continue;
        }
        let body =
            line[at..].trim_start_matches(|c: char| c.is_whitespace() || "/*!#-<".contains(c));
        for marker in ["TODO", "FIXME"] {
            let Some(after) = body.strip_prefix(marker) else {
                continue;
            };
            if after
                .chars()
                .next()
                .is_some_and(|c| c.is_alphanumeric() || c == '_')
            {
                continue;
            }
            let issue = after
                .strip_prefix('(')
                .and_then(|a| a.split(')').next())
                .and_then(|inner| inner.split('#').nth(1))
                .map(|num| {
                    num.chars()
                        .take_while(char::is_ascii_digit)
                        .collect::<String>()
                })
                .and_then(|d| d.parse().ok());
            return Some(issue);
        }
    }
    None
}

/// Scan sources for TODO/FIXME. `closed` is the set of closed issue numbers,
/// or `None` when it is unknown (then only the missing-ref check runs).
pub fn scan_todos(root: &Path, closed: Option<&BTreeSet<u64>>, issues: &mut Vec<DriftIssue>) {
    for entry in WalkBuilder::new(root).build().flatten() {
        let path = entry.path();
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if !TODO_EXTENSIONS.contains(&ext) || !path.is_file() {
            continue;
        }
        let Ok(content) = std::fs::read_to_string(path) else {
            continue;
        };
        for (i, line) in content.lines().enumerate() {
            let at = || format!("{}:{}", rel(root, path), i + 1);
            match todo_on_line(line) {
                Some(None) => add(
                    issues,
                    TODO_ISSUE,
                    at(),
                    "TODO/FIXME without an issue ref; write TODO(#123)".to_string(),
                    DriftSeverity::Warning,
                ),
                Some(Some(n)) if closed.is_some_and(|c| c.contains(&n)) => add(
                    issues,
                    TODO_ISSUE,
                    at(),
                    format!("TODO(#{n}) points at a closed issue"),
                    DriftSeverity::Warning,
                ),
                _ => {}
            }
        }
    }
}

/// Closed issue numbers via `gh`, or `None` (with a warning) if `gh` is missing or offline.
fn closed_issues(root: &Path) -> Option<BTreeSet<u64>> {
    let out = Command::new("gh")
        .args(["issue", "list", "--state", "closed", "--limit", "1000"])
        .args(["--json", "number", "--jq", ".[].number"])
        .current_dir(root)
        .output();
    match out {
        Ok(o) if o.status.success() => Some(
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .filter_map(|l| l.trim().parse().ok())
                .collect(),
        ),
        _ => {
            tracing::warn!(
                "todo-issue: `gh` is missing or offline; skipping the closed-issue check"
            );
            None
        }
    }
}

pub fn check_todo_issue(root: &Path, _config: &Config, issues: &mut Vec<DriftIssue>) {
    let mut found = Vec::new();
    scan_todos(root, None, &mut found);
    // Only ask GitHub when some TODO carries an issue ref.
    let has_refs = WalkBuilder::new(root).build().flatten().any(|e| {
        e.path().is_file()
            && e.path()
                .extension()
                .and_then(|x| x.to_str())
                .is_some_and(|x| TODO_EXTENSIONS.contains(&x))
            && std::fs::read_to_string(e.path())
                .map(|c| c.lines().any(|l| matches!(todo_on_line(l), Some(Some(_)))))
                .unwrap_or(false)
    });
    if has_refs {
        let closed = closed_issues(root);
        found.clear();
        scan_todos(root, closed.as_ref(), &mut found);
    }
    issues.append(&mut found);
}

// ------------------------------------------------------------- cited-paths

/// Parse an inline span as a `path[:line]` citation, or `None` if it is not one.
fn parse_path_ref(span: &str) -> Option<(String, Option<usize>)> {
    if span.contains(char::is_whitespace)
        || span.contains("://")
        || has_placeholder(span)
        || span.contains([
            '?', '[', ']', '|', '&', ';', '=', '"', '\'', '(', ')', ',', '@', '~', '\\',
        ])
        || span.starts_with(['/', '#'])
    {
        return None;
    }
    let (path, line) = match span.rsplit_once(':') {
        Some((p, l)) if !l.is_empty() && l.chars().next().is_some_and(|c| c.is_ascii_digit()) => {
            let first: String = l.chars().take_while(char::is_ascii_digit).collect();
            (p, first.parse().ok())
        }
        Some(_) => return None,
        None => (span, None),
    };
    if !path.contains('/') {
        return None;
    }
    let first = path.split('/').next().unwrap_or("");
    // `example.com/x.rs` is a URL-ish thing; `.github/x` and `../x` are paths.
    if first.contains('.') && !first.starts_with('.') {
        return None;
    }
    let last = path.rsplit('/').next().unwrap_or("");
    let ext_like = last.rsplit_once('.').is_some_and(|(stem, ext)| {
        !stem.is_empty()
            && (1..=5).contains(&ext.len())
            && ext.chars().all(|c| c.is_ascii_alphanumeric())
    });
    if !(path.ends_with('/') || ext_like) {
        return None;
    }
    Some((path.to_string(), line))
}

fn gitignore_for(root: &Path) -> Gitignore {
    let mut b = GitignoreBuilder::new(root);
    let _ = b.add(root.join(".gitignore"));
    b.build().unwrap_or_else(|_| Gitignore::empty())
}

/// Every path in the repo, plus each suffix that starts after a `/`, so a
/// citation like `commands/chat.rs` still resolves to `app/src/commands/chat.rs`.
fn path_suffixes(root: &Path) -> BTreeSet<String> {
    let mut set = BTreeSet::new();
    for entry in WalkBuilder::new(root).build().flatten() {
        let rel = rel(root, entry.path());
        let mut rest = rel.as_str();
        loop {
            set.insert(rest.to_string());
            match rest.split_once('/') {
                Some((_, tail)) => rest = tail,
                None => break,
            }
        }
    }
    set
}

pub fn check_cited_paths(root: &Path, config: &Config, issues: &mut Vec<DriftIssue>) {
    let ignored = gitignore_for(root);
    let suffixes = path_suffixes(root);
    let skip_docs: Vec<glob::Pattern> = config
        .drift
        .ignore_paths
        .iter()
        .filter_map(|g| glob::Pattern::new(g).ok())
        .collect();
    for doc in markdown_files(root, true) {
        if skip_docs.iter().any(|g| g.matches(&rel(root, &doc))) {
            continue;
        }
        let mut reported: BTreeSet<String> = BTreeSet::new();
        let Ok(content) = std::fs::read_to_string(&doc) else {
            continue;
        };
        let doc_dir = doc.parent().unwrap_or(root);
        for seg in md_segments(&content)
            .iter()
            .filter(|s| s.kind == Kind::Inline)
        {
            let Some((path, line)) = parse_path_ref(&seg.text) else {
                continue;
            };
            if path.starts_with(".hief/hief.db")
                || ignored
                    .matched_path_or_any_parents(&path, path.ends_with('/'))
                    .is_ignore()
            {
                continue;
            }
            let found = [root.join(&path), doc_dir.join(&path)]
                .into_iter()
                .find(|p| p.exists());
            let at = format!("{}:{}", rel(root, &doc), seg.line);
            match (found, line) {
                (None, _) if suffixes.contains(path.trim_end_matches('/')) => {}
                // Report each missing path once per document.
                (None, _) if !reported.insert(path.clone()) => {}
                (None, _) => add(
                    issues,
                    CITED_PATHS,
                    at,
                    format!("cited path `{path}` does not exist"),
                    DriftSeverity::Warning,
                ),
                (Some(p), Some(n)) if p.is_file() => {
                    if let Ok(text) = std::fs::read_to_string(&p) {
                        let total = text.lines().count();
                        if n > total {
                            add(
                                issues,
                                CITED_PATHS,
                                at,
                                format!("`{path}:{n}` is past the end of the file ({total} lines)"),
                                DriftSeverity::Warning,
                            );
                        }
                    }
                }
                _ => {}
            }
        }
    }
}

// ------------------------------------------------------ reference-freshness

#[derive(Debug, PartialEq, Eq)]
struct FrontMatter {
    verified_at: String,
    covers: Vec<String>,
}

/// Read `verified_at` and `covers` from `---` front matter (inline `[a, b]` or `- a` lists).
fn parse_front_matter(content: &str) -> Option<FrontMatter> {
    let mut lines = content.lines();
    if lines.next()?.trim() != "---" {
        return None;
    }
    let mut verified_at = None;
    let mut covers = Vec::new();
    let mut in_covers = false;
    for line in lines {
        if line.trim() == "---" {
            break;
        }
        let unquote = |s: &str| s.trim().trim_matches(['"', '\'']).to_string();
        if let Some(v) = line.strip_prefix("verified_at:") {
            verified_at = Some(unquote(v));
            in_covers = false;
        } else if let Some(v) = line.strip_prefix("covers:") {
            let v = v.trim();
            if let Some(inner) = v.strip_prefix('[').and_then(|v| v.strip_suffix(']')) {
                covers.extend(inner.split(',').map(unquote).filter(|s| !s.is_empty()));
                in_covers = false;
            } else {
                in_covers = v.is_empty();
            }
        } else if in_covers && line.trim_start().starts_with("- ") {
            covers.push(unquote(line.trim_start().trim_start_matches("- ")));
        } else if !line.starts_with([' ', '\t']) {
            in_covers = false;
        }
    }
    let verified_at = verified_at.filter(|v| !v.is_empty())?;
    (!covers.is_empty()).then_some(FrontMatter {
        verified_at,
        covers,
    })
}

fn git(root: &Path, args: &[&str]) -> Option<String> {
    let out = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

pub fn check_reference_freshness(root: &Path, _config: &Config, issues: &mut Vec<DriftIssue>) {
    if git(root, &["rev-parse", "--git-dir"]).is_none() {
        return;
    }
    for doc in markdown_files(root, true) {
        let Ok(content) = std::fs::read_to_string(&doc) else {
            continue;
        };
        let Some(fm) = parse_front_matter(&content) else {
            continue;
        };
        let at = rel(root, &doc);
        // A date has dashes, so it never reads as hex.
        let is_sha = (7..=40).contains(&fm.verified_at.len())
            && fm.verified_at.chars().all(|c| c.is_ascii_hexdigit());
        let range_or_since = if is_sha {
            if git(
                root,
                &["cat-file", "-e", &format!("{}^{{commit}}", fm.verified_at)],
            )
            .is_none()
            {
                add(
                    issues,
                    REFERENCE_FRESHNESS,
                    at,
                    format!(
                        "verified_at `{}` is not a commit in this repo",
                        fm.verified_at
                    ),
                    DriftSeverity::Warning,
                );
                continue;
            }
            format!("{}..HEAD", fm.verified_at)
        } else {
            format!("--since={}", fm.verified_at)
        };
        let mut args = vec![
            "log".to_string(),
            "--name-only".into(),
            "--format=".into(),
            range_or_since,
        ];
        args.push("--".into());
        args.extend(fm.covers.iter().map(|g| format!(":(glob){g}")));
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        let Some(out) = git(root, &args) else {
            continue;
        };
        let changed: BTreeSet<&str> = out.lines().filter(|l| !l.is_empty()).collect();
        if !changed.is_empty() {
            let shown: Vec<&str> = changed.iter().copied().take(3).collect();
            let more = changed.len() - shown.len();
            add(
                issues,
                REFERENCE_FRESHNESS,
                at,
                format!(
                    "covered files changed since verified_at {}: {}{}",
                    fm.verified_at,
                    shown.join(", "),
                    if more > 0 {
                        format!(" (+{more} more)")
                    } else {
                        String::new()
                    }
                ),
                DriftSeverity::Warning,
            );
        }
    }
}

// ----------------------------------------------------- instruction-literals

const CARGO_BUILTINS: &[&str] = &[
    "add",
    "bench",
    "build",
    "b",
    "check",
    "c",
    "clean",
    "clippy",
    "doc",
    "d",
    "fetch",
    "fix",
    "fmt",
    "generate-lockfile",
    "help",
    "info",
    "init",
    "install",
    "locate-project",
    "login",
    "metadata",
    "new",
    "owner",
    "package",
    "pkgid",
    "publish",
    "remove",
    "report",
    "run",
    "r",
    "rustc",
    "rustdoc",
    "search",
    "test",
    "t",
    "tree",
    "uninstall",
    "update",
    "vendor",
    "verify-project",
    "version",
    "yank",
    "miri",
];

fn cargo_aliases(root: &Path) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for name in [".cargo/config.toml", ".cargo/config"] {
        if let Ok(text) = std::fs::read_to_string(root.join(name))
            && let Ok(value) = text.parse::<toml::Table>()
            && let Some(aliases) = value.get("alias").and_then(|a| a.as_table())
        {
            out.extend(aliases.keys().cloned());
        }
    }
    out
}

fn on_path(binary: &str) -> bool {
    let mut dirs: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect())
        .unwrap_or_default();
    if let Some(home) = std::env::var_os("HOME") {
        dirs.push(PathBuf::from(home).join(".cargo/bin"));
    }
    dirs.iter()
        .any(|d| d.join(binary).is_file() || d.join(format!("{binary}.exe")).is_file())
}

/// Names of the Cargo packages in the root manifest and its workspace members.
fn workspace_packages(root: &Path) -> Option<BTreeSet<String>> {
    let manifest: toml::Table = std::fs::read_to_string(root.join("Cargo.toml"))
        .ok()?
        .parse()
        .ok()?;
    let mut names = BTreeSet::new();
    let name_of = |t: &toml::Table| t.get("package")?.get("name")?.as_str().map(str::to_string);
    names.extend(name_of(&manifest));
    let members = manifest
        .get("workspace")
        .and_then(|w| w.get("members"))
        .and_then(|m| m.as_array())
        .into_iter()
        .flatten()
        .filter_map(|m| m.as_str());
    for member in members {
        for dir in glob::glob(&root.join(member).to_string_lossy())
            .ok()?
            .flatten()
        {
            if let Ok(text) = std::fs::read_to_string(dir.join("Cargo.toml"))
                && let Ok(t) = text.parse::<toml::Table>()
            {
                names.extend(name_of(&t));
            }
        }
    }
    Some(names)
}

fn npm_scripts(root: &Path) -> Option<BTreeSet<String>> {
    let text = std::fs::read_to_string(root.join("package.json")).ok()?;
    let json: serde_json::Value = serde_json::from_str(&text).ok()?;
    Some(
        json.get("scripts")
            .and_then(|s| s.as_object())
            .map(|o| o.keys().cloned().collect())
            .unwrap_or_default(),
    )
}

/// Split a shell line into simple commands.
fn commands_of(text: &str) -> Vec<Vec<&str>> {
    let text = text.trim_start_matches(['$', '>']).trim();
    text.split(['&', '|', ';'])
        .map(|part| part.split_whitespace().collect::<Vec<_>>())
        .filter(|t| !t.is_empty())
        .collect()
}

pub fn check_instruction_literals(root: &Path, _config: &Config, issues: &mut Vec<DriftIssue>) {
    let aliases = cargo_aliases(root);
    let packages = workspace_packages(root);
    let scripts = npm_scripts(root);
    let mut missing_pkg_json_reported = false;

    for name in INSTRUCTION_FILES {
        let doc = root.join(name);
        let Ok(content) = std::fs::read_to_string(&doc) else {
            continue;
        };
        for seg in md_segments(&content) {
            let text = seg.text.trim_start_matches(['$', '>']).trim();
            if has_placeholder(text) {
                continue;
            }
            for tokens in commands_of(text) {
                let at = format!("{name}:{}", seg.line);
                let first = tokens[0];
                // Inline spans only count when they clearly start with a command.
                let looks_like_command = matches!(
                    first,
                    "npm" | "pnpm" | "yarn" | "bun" | "cargo" | "bash" | "sh"
                ) || first.starts_with("scripts/")
                    || first.starts_with("./scripts/");
                if seg.kind == Kind::Inline
                    && (!looks_like_command || tokens.len() < 2 && !first.contains('/'))
                {
                    continue;
                }

                // scripts/foo.sh, ./scripts/foo.sh, bash scripts/foo.sh
                let script = tokens
                    .iter()
                    .take(2)
                    .find(|t| t.trim_start_matches("./").starts_with("scripts/"));
                if let Some(script) = script {
                    let p = script.trim_start_matches("./");
                    if !root.join(p).exists() {
                        add(
                            issues,
                            INSTRUCTION_LITERALS,
                            at.clone(),
                            format!("script `{p}` does not exist"),
                            DriftSeverity::Warning,
                        );
                    }
                }

                match first {
                    "npm" | "pnpm" | "yarn" | "bun" => {
                        let script_name = match tokens.get(1).copied() {
                            Some("run" | "run-script") => tokens.get(2).copied(),
                            Some("test") if first == "npm" => Some("test"),
                            Some("start") if first == "npm" => Some("start"),
                            _ => None,
                        };
                        let Some(script_name) = script_name else {
                            continue;
                        };
                        match &scripts {
                            None if !missing_pkg_json_reported => {
                                missing_pkg_json_reported = true;
                                add(
                                    issues,
                                    INSTRUCTION_LITERALS,
                                    at,
                                    format!(
                                        "`{first} ... {script_name}` is used but there is no package.json"
                                    ),
                                    DriftSeverity::Warning,
                                );
                            }
                            Some(s) if !s.contains(script_name) => add(
                                issues,
                                INSTRUCTION_LITERALS,
                                at,
                                format!("package.json has no script `{script_name}`"),
                                DriftSeverity::Warning,
                            ),
                            _ => {}
                        }
                    }
                    "cargo" => {
                        let mut rest = tokens.iter().skip(1).filter(|t| !t.starts_with('+'));
                        if let Some(sub) = rest.next().filter(|s| !s.starts_with('-')) {
                            let known = CARGO_BUILTINS.contains(sub)
                                || aliases.contains(*sub)
                                || on_path(&format!("cargo-{sub}"));
                            if !known {
                                add(
                                    issues,
                                    INSTRUCTION_LITERALS,
                                    at.clone(),
                                    format!(
                                        "`cargo {sub}` is not a cargo command, alias or installed subcommand"
                                    ),
                                    DriftSeverity::Info,
                                );
                            }
                        }
                        if let Some(pkgs) = packages.as_ref().filter(|p| !p.is_empty()) {
                            for pair in tokens.windows(2) {
                                if matches!(pair[0], "-p" | "--package") && !pkgs.contains(pair[1])
                                {
                                    add(
                                        issues,
                                        INSTRUCTION_LITERALS,
                                        at.clone(),
                                        format!(
                                            "cargo package `{}` is not in this workspace",
                                            pair[1]
                                        ),
                                        DriftSeverity::Warning,
                                    );
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(root: &Path, path: &str, body: &str) {
        let p = root.join(path);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, body).unwrap();
    }

    fn run(f: fn(&Path, &Config, &mut Vec<DriftIssue>), root: &Path) -> Vec<DriftIssue> {
        let mut issues = Vec::new();
        f(root, &Config::default(), &mut issues);
        issues
    }

    // ---- todo-issue

    #[test]
    fn todo_parsing() {
        assert_eq!(todo_on_line("// TODO fix this"), Some(None));
        assert_eq!(todo_on_line("# FIXME: later"), Some(None));
        assert_eq!(todo_on_line("// TODO(#12): fix"), Some(Some(12)));
        assert_eq!(todo_on_line("/* FIXME(#3) */"), Some(Some(3)));
        // not a comment, or not the bare word
        assert_eq!(todo_on_line("let todo = TODO_LIST;"), None);
        assert_eq!(todo_on_line("// TODOS are tracked elsewhere"), None);
        assert_eq!(todo_on_line("// MYTODO"), None);
    }

    #[test]
    fn todo_checker_flags_missing_and_closed_refs() {
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            "src/a.rs",
            "fn a() {}\n// TODO tidy\n// TODO(#7) ok\n// TODO(#9) stale\n",
        );
        let closed: BTreeSet<u64> = [9].into();
        let mut issues = Vec::new();
        scan_todos(dir.path(), Some(&closed), &mut issues);
        let msgs: Vec<_> = issues
            .iter()
            .map(|i| (i.file.as_str(), i.message.as_str()))
            .collect();
        assert_eq!(msgs.len(), 2, "{msgs:?}");
        assert!(msgs[0].0 == "src/a.rs:2" && msgs[0].1.contains("without an issue ref"));
        assert!(msgs[1].0 == "src/a.rs:4" && msgs[1].1.contains("closed"));
    }

    #[test]
    fn todo_checker_without_gh_data_only_checks_refs() {
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            "a.py",
            "# TODO(#9) maybe closed\nx = 1  # no marker\n",
        );
        let mut issues = Vec::new();
        scan_todos(dir.path(), None, &mut issues);
        assert!(issues.is_empty());
    }

    // ---- cited-paths

    #[test]
    fn path_ref_parsing_ignores_non_paths() {
        assert_eq!(
            parse_path_ref("src/db.rs:12"),
            Some(("src/db.rs".into(), Some(12)))
        );
        assert_eq!(
            parse_path_ref("docs/specs/"),
            Some(("docs/specs/".into(), None))
        );
        for no in [
            "https://example.com/a.rs",
            "src/**/*.rs",
            "and/or",
            "application/json",
            "/etc/hosts.conf",
            "github.com/org/repo.git",
            "src/<name>.rs",
            "Cargo.toml",
            "cargo build --release",
            "a/b.rs:xyz",
        ] {
            assert_eq!(parse_path_ref(no), None, "{no}");
        }
    }

    #[test]
    fn cited_paths_flags_missing_files_and_lines_past_the_end() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "src/real.rs", "a\nb\nc\n");
        write(dir.path(), ".gitignore", "target/\n");
        write(
            dir.path(),
            "AGENTS.md",
            "See `src/real.rs:2` and `src/real.rs:99`, `src/gone.rs`.\n\
             Ignored: `target/debug/hief`, https://x.io/a.rs, `src/*.rs`.\n\
             ```text example\n`src/also_gone.rs`\n```\n\
             ```\nsrc/in_fence.rs\n```\n",
        );
        let issues = run(check_cited_paths, dir.path());
        let msgs: Vec<_> = issues.iter().map(|i| i.message.as_str()).collect();
        assert_eq!(msgs.len(), 2, "{msgs:?}");
        assert!(msgs[0].contains("past the end"));
        assert!(msgs[1].contains("src/gone.rs"));
        assert_eq!(issues[0].file, "AGENTS.md:1");
    }

    // ---- reference-freshness

    #[test]
    fn front_matter_parsing() {
        let inline = "---\nverified_at: abc1234\ncovers: [src/**, \"Cargo.toml\"]\n---\nbody";
        assert_eq!(
            parse_front_matter(inline),
            Some(FrontMatter {
                verified_at: "abc1234".into(),
                covers: vec!["src/**".into(), "Cargo.toml".into()]
            })
        );
        let block = "---\ntitle: x\ncovers:\n  - a/*.rs\n  - b.rs\nverified_at: 2026-01-01\n---\n";
        let fm = parse_front_matter(block).unwrap();
        assert_eq!(fm.covers, vec!["a/*.rs", "b.rs"]);
        assert_eq!(fm.verified_at, "2026-01-01");
        assert_eq!(parse_front_matter("no front matter"), None);
        assert_eq!(parse_front_matter("---\nverified_at: abc1234\n---\n"), None);
    }

    fn git_in(root: &Path, args: &[&str]) {
        let ok = Command::new("git")
            .args(["-c", "user.email=t@t", "-c", "user.name=t"])
            .args(args)
            .current_dir(root)
            .status()
            .unwrap()
            .success();
        assert!(ok, "git {args:?}");
    }

    #[test]
    fn reference_freshness_flags_covered_files_changed_after_verified_at() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        git_in(root, &["init", "-q", "-b", "main"]);
        write(root, "src/a.rs", "1");
        write(root, "other.txt", "1");
        git_in(root, &["add", "."]);
        git_in(root, &["commit", "-qm", "one"]);
        let sha = String::from_utf8(
            Command::new("git")
                .args(["rev-parse", "--short=10", "HEAD"])
                .current_dir(root)
                .output()
                .unwrap()
                .stdout,
        )
        .unwrap();
        let sha = sha.trim();
        write(
            root,
            "docs/ref.md",
            &format!("---\nverified_at: {sha}\ncovers: [src/**]\n---\n# Ref\n"),
        );
        write(
            root,
            "docs/bad.md",
            "---\nverified_at: deadbeef0\ncovers: [src/**]\n---\n",
        );
        git_in(root, &["add", "."]);
        git_in(root, &["commit", "-qm", "docs"]);
        assert_eq!(
            run(check_reference_freshness, root).len(),
            1,
            "only the bad sha so far"
        );

        write(root, "other.txt", "2");
        git_in(root, &["commit", "-qam", "unrelated"]);
        assert_eq!(
            run(check_reference_freshness, root).len(),
            1,
            "unrelated change is not drift"
        );

        write(root, "src/a.rs", "2");
        git_in(root, &["commit", "-qam", "covered"]);
        let issues = run(check_reference_freshness, root);
        assert_eq!(issues.len(), 2);
        assert!(
            issues
                .iter()
                .any(|i| i.file == "docs/ref.md" && i.message.contains("src/a.rs"))
        );
        assert!(
            issues
                .iter()
                .any(|i| i.file == "docs/bad.md" && i.message.contains("not a commit"))
        );
    }

    #[test]
    fn reference_freshness_outside_a_git_repo_is_silent() {
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            "docs/ref.md",
            "---\nverified_at: 2020-01-01\ncovers: [src/**]\n---\n",
        );
        assert!(run(check_reference_freshness, dir.path()).is_empty());
    }

    // ---- instruction-literals

    #[test]
    fn instruction_literals_flag_unresolvable_commands() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        write(root, "package.json", r#"{"scripts":{"build":"x"}}"#);
        write(
            root,
            "Cargo.toml",
            "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n",
        );
        write(root, ".cargo/config.toml", "[alias]\nxt = \"test\"\n");
        write(root, "scripts/ok.sh", "");
        write(
            root,
            "AGENTS.md",
            "Run `npm run build` then `npm run nope`.\n\
             Use `cargo xt`, `cargo build -p demo`, `cargo build -p ghost`.\n\
             `scripts/ok.sh` and `scripts/gone.sh`.\n\
             ```sh\n$ cargo test && ./scripts/gone2.sh\nnpm run <name>\n```\n\
             ```sh example\nnpm run example-only\n```\n\
             Prose with npm run in it, and `cargo` alone.\n",
        );
        let msgs: Vec<_> = run(check_instruction_literals, root)
            .into_iter()
            .map(|i| i.message)
            .collect();
        assert_eq!(msgs.len(), 4, "{msgs:?}");
        assert!(msgs.iter().any(|m| m.contains("no script `nope`")));
        assert!(msgs.iter().any(|m| m.contains("package `ghost`")));
        assert!(msgs.iter().any(|m| m.contains("scripts/gone.sh")));
        assert!(msgs.iter().any(|m| m.contains("scripts/gone2.sh")));
    }

    #[test]
    fn disabled_checkers_are_skipped() {
        let dir = tempfile::tempdir().unwrap();
        let mut cfg = Config::default();
        cfg.drift.disabled_checkers = vec![TODO_ISSUE.into(), CITED_PATHS.into()];
        let report = crate::drift::run(dir.path(), &cfg).unwrap();
        assert_eq!(report.checks_run.len(), 10);
        assert!(!report.checks_run.contains(&TODO_ISSUE.to_string()));
    }
}
