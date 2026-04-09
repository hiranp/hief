use std::path::Path;
use std::process::Command;

use serde_json::Value;

fn write_file(path: &Path, content: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("create parent dirs");
    }
    std::fs::write(path, content).expect("write file");
}

fn run_hief(cwd: &Path, args: &[&str]) -> std::process::Output {
    let bin = env!("CARGO_BIN_EXE_hief");
    Command::new(bin)
        .current_dir(cwd)
        .env("RUST_LOG", "off")
        .args(args)
        .output()
        .expect("run hief")
}

fn init_git_repo(root: &Path) {
    let run = |args: &[&str]| {
        let out = Command::new("git")
            .current_dir(root)
            .args(args)
            .output()
            .expect("run git");
        assert!(
            out.status.success(),
            "git command failed: {:?}\nstdout={}\nstderr={}",
            args,
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
    };

    run(&["init", "-q"]);
    run(&["config", "user.email", "eval-tests@example.com"]);
    run(&["config", "user.name", "Eval Tests"]);
}

fn git_commit_all(root: &Path, message: &str) {
    let out_add = Command::new("git")
        .current_dir(root)
        .args(["add", "."])
        .output()
        .expect("git add");
    assert!(
        out_add.status.success(),
        "git add failed: {}",
        String::from_utf8_lossy(&out_add.stderr)
    );

    let out_commit = Command::new("git")
        .current_dir(root)
        .args(["commit", "-m", message])
        .output()
        .expect("git commit");
    assert!(
        out_commit.status.success(),
        "git commit failed: {}",
        String::from_utf8_lossy(&out_commit.stderr)
    );
}

#[test]
fn test_eval_run_persists_history_and_config_loads() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path();

    write_file(
        &root.join("hief.toml"),
        r#"
[hief]
version = "0.2.8"
"#,
    );

    write_file(
        &root.join(".hief/golden/workflow.toml"),
        r#"
[metadata]
name = "workflow"
description = "integration workflow"

[[cases]]
id = "c1"
name = "no forbidden token"
priority = "high"
[cases.checks]
must_not_contain = ["FORBIDDEN_TOKEN"]
file_patterns = ["src/*.rs"]
"#,
    );

    write_file(
        &root.join("src/lib.rs"),
        "pub fn hello() -> &'static str { \"ok\" }\n",
    );

    init_git_repo(root);
    git_commit_all(root, "initial");

    let out_index = run_hief(root, &["index", "build"]);
    assert!(
        out_index.status.success(),
        "index build failed: {}",
        String::from_utf8_lossy(&out_index.stderr)
    );

    let out_eval = run_hief(root, &["--json", "eval", "run", "--golden", "workflow"]);
    assert!(
        out_eval.status.success(),
        "eval run failed: {}",
        String::from_utf8_lossy(&out_eval.stderr)
    );

    let results: Value =
        serde_json::from_slice(&out_eval.stdout).expect("parse eval run --json output");
    let first = &results[0];
    assert_eq!(first["golden_set"], "workflow");
    assert_eq!(first["passed"], true);

    let out_report = run_hief(
        root,
        &["--json", "eval", "report", "workflow", "--limit", "5"],
    );
    assert!(
        out_report.status.success(),
        "eval report failed: {}",
        String::from_utf8_lossy(&out_report.stderr)
    );

    let history: Value =
        serde_json::from_slice(&out_report.stdout).expect("parse eval report --json output");
    assert!(history.as_array().is_some_and(|arr| !arr.is_empty()));
}

#[test]
fn test_eval_run_ci_returns_nonzero_on_failure() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path();

    write_file(
        &root.join("hief.toml"),
        r#"
[hief]
version = "0.2.8"
"#,
    );

    write_file(
        &root.join(".hief/golden/failing.toml"),
        r#"
[metadata]
name = "failing"
description = "must contain an impossible token"

[[cases]]
id = "c1"
name = "missing required literal"
priority = "critical"
[cases.checks]
must_contain = ["THIS_TOKEN_DOES_NOT_EXIST"]
file_patterns = ["src/*.rs"]
"#,
    );

    write_file(
        &root.join("src/lib.rs"),
        "pub fn hello() -> &'static str { \"ok\" }\n",
    );

    init_git_repo(root);
    git_commit_all(root, "initial");

    let out_index = run_hief(root, &["index", "build"]);
    assert!(out_index.status.success());

    let out_ci = run_hief(root, &["eval", "run", "--golden", "failing", "--ci"]);
    assert!(
        !out_ci.status.success(),
        "ci mode should fail for critical missing check"
    );
}

#[test]
fn test_diff_only_scopes_to_changed_files_since_last_eval() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path();

    write_file(
        &root.join("hief.toml"),
        r#"
[hief]
version = "0.2.8"
"#,
    );

    write_file(
        &root.join(".hief/golden/diffset.toml"),
        r#"
[metadata]
name = "diffset"
description = "diff only checks"

[[cases]]
id = "c1"
name = "forbidden marker in changed files"
priority = "high"
[cases.checks]
must_not_contain = ["FORBIDDEN_MARKER"]
file_patterns = ["src/*.rs"]
diff_only = true
"#,
    );

    write_file(&root.join("src/a.rs"), "pub fn a() { println!(\"a\"); }\n");
    write_file(
        &root.join("src/b.rs"),
        "pub fn b() { let _ = \"FORBIDDEN_MARKER\"; }\n",
    );

    init_git_repo(root);
    git_commit_all(root, "baseline");

    let out_index_1 = run_hief(root, &["index", "build"]);
    assert!(out_index_1.status.success());

    // First run has no eval history baseline; expected full-scope behavior (fails).
    let out_eval_1 = run_hief(root, &["--json", "eval", "run", "--golden", "diffset"]);
    assert!(out_eval_1.status.success());
    let first_results: Value =
        serde_json::from_slice(&out_eval_1.stdout).expect("parse first eval json");
    assert_eq!(first_results[0]["passed"], false);
    assert_eq!(first_results[0]["scope"]["mode"], "full");

    // Change only src/a.rs, commit, and rebuild index.
    write_file(
        &root.join("src/a.rs"),
        "pub fn a() { println!(\"a changed\"); }\n",
    );
    git_commit_all(root, "change only a");

    let out_index_2 = run_hief(root, &["index", "build"]);
    assert!(out_index_2.status.success());

    let out_eval_2 = run_hief(root, &["--json", "eval", "run", "--golden", "diffset"]);
    assert!(out_eval_2.status.success());
    let second_results: Value =
        serde_json::from_slice(&out_eval_2.stdout).expect("parse second eval json");

    let second = &second_results[0];
    assert_eq!(second["scope"]["mode"], "diff");
    assert!(second["scope"]["base_commit"].is_string());
    assert_eq!(
        second["passed"], true,
        "unchanged forbidden file should be ignored in diff mode"
    );
}
