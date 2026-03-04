//! `hief upgrade` — compatibility alias for post-upgrade fixes.

use std::path::Path;

use crate::config::Config;
use crate::db::Database;
use crate::errors::Result;

/// Upgrade workflow (runtime-safe):
/// 1. Load current config/db
/// 2. Run doctor with `--fix` semantics to apply safe migrations/fixes
///
/// Binary distribution upgrades should be handled externally (package manager,
/// installer, or manual binary replacement). This command is intentionally a
/// maintenance alias and does not compile source code.
pub async fn upgrade(project_root: &Path, config_path: &Path, json: bool) -> Result<()> {
    if !json {
        println!("ℹ️  `hief upgrade` runs maintenance fixes only.");
        println!("   Use your package/distribution channel to install new binaries.");
    }

    let config = Config::load(config_path)?;
    let db_path = Config::db_path(project_root);
    let db = Database::open(&db_path).await?;

    crate::cli::commands::doctor(&db, project_root, config_path, &config, true, json).await?;

    if !json {
        println!("✅ Maintenance upgrade complete (doctor --fix applied)");
    }

    Ok(())
}
