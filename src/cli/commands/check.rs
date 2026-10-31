use std::path::Path;

use crate::cli::CheckFormat;
use crate::config::Config;
use crate::drift::DriftSeverity;
use crate::drift::baseline;
use crate::errors::Result;

/// Options for `hief check`.
pub struct CheckOptions {
    /// Print only the one-line score summary.
    pub quiet: bool,
    /// Output format.
    pub format: CheckFormat,
    /// Write the current issues as the new baseline and exit 0.
    pub write_baseline: bool,
    /// Ignore `.hief/drift-baseline.json`.
    pub no_baseline: bool,
    /// Only count issues in files changed since this git ref.
    pub diff_base: Option<String>,
}

/// Run drift detection and print the report to stdout.
///
/// Returns the suggested exit code: 0 = clean, 1 = new errors found. Usage and
/// config problems come back as `Err`, which `main` turns into exit code 2.
/// Needs no index: nothing here opens `.hief/hief.db`.
pub async fn run_check(project_root: &Path, config: &Config, opts: &CheckOptions) -> Result<i32> {
    let mut report = crate::drift::run(project_root, config)?;

    if opts.write_baseline {
        let n = baseline::write(project_root, &report)?;
        println!("hief: wrote {n} issue(s) to {}", baseline::BASELINE_PATH);
        return Ok(0);
    }

    let mut suppressed = 0;
    if !opts.no_baseline {
        (report, suppressed) = baseline::apply(project_root, report)?;
    }
    if let Some(base) = &opts.diff_base {
        report = baseline::diff_scope(project_root, report, base)?;
    }
    let exit_code = if report.error_count > 0 { 1 } else { 0 };

    match opts.format {
        CheckFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&report).unwrap_or_default()
            );
            return Ok(exit_code);
        }
        CheckFormat::Github => {
            for issue in &report.issues {
                let level = match issue.severity {
                    DriftSeverity::Error => "error",
                    DriftSeverity::Warning => "warning",
                    DriftSeverity::Info => "notice",
                };
                let line = baseline::line_of(&issue.file).unwrap_or(1);
                println!(
                    "::{level} file={},line={line},title=hief {}::{}",
                    github_escape_prop(baseline::file_without_line(&issue.file)),
                    github_escape_prop(&issue.checker),
                    github_escape_data(&issue.message),
                );
            }
            return Ok(exit_code);
        }
        CheckFormat::Text => {}
    }

    if opts.quiet {
        println!(
            "hief: drift score {}/100 ({} error{}, {} warning{})",
            report.score,
            report.error_count,
            if report.error_count == 1 { "" } else { "s" },
            report.warning_count,
            if report.warning_count == 1 { "" } else { "s" },
        );
        return Ok(exit_code);
    }

    // Full report
    println!();
    let score_label = if report.score == 100 {
        format!("\x1b[32m✅ {}/100\x1b[0m", report.score)
    } else if report.score >= 80 {
        format!("\x1b[33m⚠  {}/100\x1b[0m", report.score)
    } else {
        format!("\x1b[31m✗  {}/100\x1b[0m", report.score)
    };

    println!("  Drift score: {}", score_label);
    println!(
        "  {} error{}, {} warning{}, {} info",
        report.error_count,
        if report.error_count == 1 { "" } else { "s" },
        report.warning_count,
        if report.warning_count == 1 { "" } else { "s" },
        report.info_count,
    );
    println!();

    if report.issues.is_empty() {
        println!("  ✓ No drift issues — scaffold is in sync with the codebase.");
    } else {
        // Group by checker
        let mut seen_checkers: Vec<String> = Vec::new();
        for issue in &report.issues {
            if !seen_checkers.contains(&issue.checker) {
                seen_checkers.push(issue.checker.clone());
            }
        }

        for checker in &seen_checkers {
            let checker_issues: Vec<_> = report
                .issues
                .iter()
                .filter(|i| &i.checker == checker)
                .collect();

            println!("  [{}]", checker);
            for issue in &checker_issues {
                let (icon, color) = match issue.severity {
                    DriftSeverity::Error => ("✗", "\x1b[31m"),
                    DriftSeverity::Warning => ("⚠", "\x1b[33m"),
                    DriftSeverity::Info => ("ℹ", "\x1b[36m"),
                };
                println!(
                    "    {}{} {}\x1b[0m — {}",
                    color, icon, issue.file, issue.message
                );
            }
            println!();
        }

        if report.error_count > 0 {
            println!("  Fix errors to bring the score back to 100.");
        }
    }

    println!(
        "  Checked {} area(s). Run with --json for machine-readable output.",
        report.checks_run.len()
    );
    println!();

    if suppressed > 0 {
        println!(
            "  {suppressed} known issue(s) hidden by {}.",
            baseline::BASELINE_PATH
        );
        println!();
    }

    Ok(exit_code)
}

/// Escape a workflow-command message (`::error ...::<data>`).
fn github_escape_data(s: &str) -> String {
    s.replace('%', "%25")
        .replace('\r', "%0D")
        .replace('\n', "%0A")
}

/// Escape a workflow-command property value.
fn github_escape_prop(s: &str) -> String {
    github_escape_data(s)
        .replace(':', "%3A")
        .replace(',', "%2C")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn github_annotations_escape_reserved_characters() {
        assert_eq!(github_escape_data("a\nb%"), "a%0Ab%25");
        assert_eq!(github_escape_prop("a,b:c"), "a%2Cb%3Ac");
    }

    #[tokio::test]
    async fn check_runs_without_an_index() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!dir.path().join(".hief/hief.db").exists());
        let opts = CheckOptions {
            quiet: true,
            format: CheckFormat::Text,
            write_baseline: false,
            no_baseline: false,
            diff_base: None,
        };
        run_check(dir.path(), &Config::default(), &opts)
            .await
            .unwrap();
        assert!(!dir.path().join(".hief/hief.db").exists());
    }

    #[tokio::test]
    async fn baseline_turns_a_failing_check_green() {
        let dir = tempfile::tempdir().unwrap();
        let opts = |write_baseline| CheckOptions {
            quiet: true,
            format: CheckFormat::Text,
            write_baseline,
            no_baseline: false,
            diff_base: None,
        };
        let cfg = Config::default();
        run_check(dir.path(), &cfg, &opts(true)).await.unwrap();
        assert_eq!(run_check(dir.path(), &cfg, &opts(false)).await.unwrap(), 0);
    }
}
