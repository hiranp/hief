//! `hief upgrade` — local binary upgrade helper.

use std::path::Path;
use std::process::Command;

use crate::config::Config;
use crate::db::Database;
use crate::errors::{HiefError, Result};

/// Upgrade workflow:
/// 1. Build release binary
/// 2. Refresh installation link at ~/bin/hief
/// 3. Run doctor with `--fix` semantics to apply safe migrations/fixes
pub async fn upgrade(project_root: &Path, config_path: &Path, json: bool) -> Result<()> {
    let release_binary = project_root.join("target").join("release").join("hief");

    run_step(
        project_root,
        "cargo",
        &["build", "--release"],
        "build_release",
        json,
    )?;

    let home = std::env::var("HOME").map_err(|_| {
        HiefError::Other("HOME is not set; cannot install upgraded binary".to_string())
    })?;
    let bin_dir = Path::new(&home).join("bin");
    std::fs::create_dir_all(&bin_dir)?;
    let install_path = bin_dir.join("hief");

    if install_path.exists() || install_path.is_symlink() {
        std::fs::remove_file(&install_path)?;
    }

    #[cfg(unix)]
    std::os::unix::fs::symlink(&release_binary, &install_path)?;

    #[cfg(not(unix))]
    {
        std::fs::copy(&release_binary, &install_path)?;
    }

    if !json {
        println!("✅ Installed upgraded binary at {}", install_path.display());
    }

    let config = Config::load(config_path)?;
    let db_path = Config::db_path(project_root);
    let db = Database::open(&db_path).await?;

    crate::cli::commands::doctor(&db, project_root, config_path, &config, true, json).await?;

    if !json {
        println!("✅ Upgrade complete");
    }

    Ok(())
}

fn run_step(
    project_root: &Path,
    program: &str,
    args: &[&str],
    step_name: &str,
    json: bool,
) -> Result<()> {
    let status = Command::new(program)
        .args(args)
        .current_dir(project_root)
        .status()
        .map_err(|e| {
            HiefError::Other(format!(
                "failed to execute {} ({:?}) for step '{}': {}",
                program, args, step_name, e
            ))
        })?;

    if !status.success() {
        return Err(HiefError::Other(format!(
            "upgrade step '{}' failed: {} {:?}",
            step_name, program, args
        )));
    }

    if !json {
        println!("✅ Step '{}' completed", step_name);
    }

    Ok(())
}
