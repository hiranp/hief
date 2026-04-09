//! `hief hooks` — git hook management commands.

use std::path::Path;

use crate::errors::Result;

// ---------------------------------------------------------------------------
// Hook script contents
// ---------------------------------------------------------------------------

const POST_COMMIT_HOOK: &str = r#"#!/bin/sh
# HIEF auto-index + drift check hook — installed by `hief hooks install`
# HIEF_HOOK_VERSION=3
# After every commit: incrementally reindex and check for documentation drift.
if command -v hief >/dev/null 2>&1; then
    # Reindex in background (non-blocking)
    hief index build --json >/dev/null 2>&1 &
    # Drift check — only prints when score < 100
    hief check --quiet 2>/dev/null | grep -v "100/100" || true
fi
"#;

const PRE_PUSH_HOOK: &str = r#"#!/bin/sh
# HIEF pre-push eval check — installed by `hief hooks install`
# HIEF_HOOK_VERSION=2
# Repairs local HIEF state, then runs evaluation checks before pushing.
if command -v hief >/dev/null 2>&1; then
    echo "🩺 Running HIEF doctor auto-fix..."
    hief doctor --fix
    doctor_exit=$?
    if [ $doctor_exit -ne 0 ]; then
        echo "❌ HIEF doctor failed — push blocked"
        exit 1
    fi

    echo "🔍 Running HIEF evaluation checks..."
    hief eval run --ci
    exit_code=$?
    if [ $exit_code -ne 0 ]; then
        echo "❌ HIEF evaluation failed — push blocked"
        exit 1
    fi
fi
"#;

/// Install HIEF git hooks.
pub fn hooks_install(project_root: &Path, json: bool) -> Result<()> {
    let hooks_dir = project_root.join(".git/hooks");

    if !hooks_dir.exists() {
        if json {
            println!(
                "{}",
                serde_json::json!({"error": "Not a git repository — .git/hooks not found"})
            );
        } else {
            println!("❌ Not a git repository — .git/hooks not found");
        }
        return Ok(());
    }

    let mut installed = Vec::new();

    // Install post-commit hook
    let post_commit_path = hooks_dir.join("post-commit");
    install_hook(
        &post_commit_path,
        POST_COMMIT_HOOK,
        "post-commit",
        &mut installed,
    )?;

    // Install pre-push hook
    let pre_push_path = hooks_dir.join("pre-push");
    install_hook(&pre_push_path, PRE_PUSH_HOOK, "pre-push", &mut installed)?;

    if json {
        println!(
            "{}",
            serde_json::json!({"installed": installed, "hooks_dir": hooks_dir.display().to_string()})
        );
    } else {
        if installed.is_empty() {
            println!("⏭️  All hooks already installed");
        } else {
            for hook_name in &installed {
                println!("✅ Installed {} hook", hook_name);
            }
        }
        println!("   Hooks directory: {}", hooks_dir.display());
    }

    Ok(())
}

/// Uninstall HIEF git hooks.
pub fn hooks_uninstall(project_root: &Path, json: bool) -> Result<()> {
    let hooks_dir = project_root.join(".git/hooks");
    let mut removed = Vec::new();

    for hook_name in &["post-commit", "pre-push"] {
        let hook_path = hooks_dir.join(hook_name);
        if hook_path.exists() {
            let content = std::fs::read_to_string(&hook_path).unwrap_or_default();
            if content.contains("hief") {
                std::fs::remove_file(&hook_path)?;
                removed.push(hook_name.to_string());
            }
        }
    }

    if json {
        println!("{}", serde_json::json!({"removed": removed}));
    } else if removed.is_empty() {
        println!("⏭️  No HIEF hooks found to remove");
    } else {
        for hook_name in &removed {
            println!("🗑️  Removed {} hook", hook_name);
        }
    }

    Ok(())
}

/// Show git hook status.
pub fn hooks_status(project_root: &Path, json: bool) -> Result<()> {
    let hooks_dir = project_root.join(".git/hooks");

    let hook_names = ["post-commit", "pre-push"];
    let mut statuses = Vec::new();

    for hook_name in &hook_names {
        let hook_path = hooks_dir.join(hook_name);
        let installed = if hook_path.exists() {
            let content = std::fs::read_to_string(&hook_path).unwrap_or_default();
            content.contains("hief")
        } else {
            false
        };
        statuses.push(serde_json::json!({
            "hook": hook_name,
            "installed": installed,
        }));
    }

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&statuses).expect("failed to serialize statuses")
        );
    } else {
        println!("🪝 Git Hook Status:\n");
        for s in &statuses {
            let name = s["hook"].as_str().expect("hook field missing");
            let installed = s["installed"].as_bool().expect("installed field missing");
            let icon = if installed { "✅" } else { "❌" };
            println!(
                "  {} {} — {}",
                icon,
                name,
                if installed {
                    "installed"
                } else {
                    "not installed"
                }
            );
        }
    }

    Ok(())
}

/// Helper: install a single hook, appending to existing if needed.
fn install_hook(path: &Path, content: &str, name: &str, installed: &mut Vec<String>) -> Result<()> {
    if path.exists() {
        let existing = std::fs::read_to_string(path)?;
        if existing.contains("hief") {
            if existing == content {
                return Ok(()); // Already at latest known content
            }
            std::fs::write(path, content)?;
            installed.push(name.to_string());
            // Make executable
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let perms = std::fs::Permissions::from_mode(0o755);
                std::fs::set_permissions(path, perms)?;
            }
            return Ok(());
        }
        // Append to existing hook
        std::fs::write(path, format!("{}\n{}", existing.trim(), content))?;
    } else {
        std::fs::write(path, content)?;
    }

    // Make executable
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o755);
        std::fs::set_permissions(path, perms)?;
    }

    installed.push(name.to_string());
    Ok(())
}
