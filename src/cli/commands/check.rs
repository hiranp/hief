//! `hief check` — documentation drift detection CLI command.
//!
//! Runs all 8 drift checkers against .hief/ and prints a human-readable report.
//! Returns exit code 1 if any errors are found (useful in CI and git hooks).

use std::path::Path;

use crate::config::Config;
use crate::drift::DriftSeverity;
use crate::errors::Result;

/// Run drift detection and print the report to stdout.
///
/// Returns the suggested exit code: 0 = clean, 1 = errors found.
pub async fn run_check(
    project_root: &Path,
    config: &Config,
    quiet: bool,
    json: bool,
) -> Result<i32> {
    let report = crate::drift::run(project_root, config)?;

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&report).unwrap_or_default()
        );
        return Ok(if report.error_count > 0 { 1 } else { 0 });
    }

    if quiet {
        println!(
            "hief: drift score {}/100 ({} error{}, {} warning{})",
            report.score,
            report.error_count,
            if report.error_count == 1 { "" } else { "s" },
            report.warning_count,
            if report.warning_count == 1 { "" } else { "s" },
        );
        return Ok(if report.error_count > 0 { 1 } else { 0 });
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

    Ok(if report.error_count > 0 { 1 } else { 0 })
}
