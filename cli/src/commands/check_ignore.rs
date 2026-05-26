use clap::Args;
use std::collections::HashSet;
use std::path::PathBuf;

/// Check whether a path would be excluded by exclude rules.
///
/// Reads `exclude_dirs` from config and `.shiotsuchiignore` from the vault root,
/// then checks whether the given relative path would be excluded.
#[derive(Args, Debug)]
pub struct CheckIgnoreArgs {
    /// Relative path to check (e.g. "private/notes.md" or "node_modules/pkg/")
    pub path: String,

    /// Vault directory (defaults to the first configured vault)
    #[arg(long)]
    pub vault: Option<String>,
}

pub fn run_check_ignore(
    args: &CheckIgnoreArgs,
    vaults: &[(String, PathBuf)],
) -> Result<(), Box<dyn std::error::Error>> {
    use shiotsuchi_core::indexer::{check_ignore, load_shiotsuchiignore};
    use shiotsuchi_core::config::ShiotsuchiConfig;

    // Determine which vault's config to use
    let vault_dir = if let Some(ref vault_id) = args.vault {
        vaults
            .iter()
            .find(|(name, _)| name == vault_id)
            .map(|(_, dir)| dir.clone())
            .ok_or_else(|| format!("Vault '{}' not found", vault_id))?
    } else if let Some((_, dir)) = vaults.first() {
        dir.clone()
    } else {
        return Err("No vaults configured".into());
    };

    // Load patterns: config exclude_dirs + .shiotsuchiignore
    let cfg = ShiotsuchiConfig::load();
    let config_patterns: Vec<String> = cfg.indexing.exclude_dirs.clone();
    let ignore_patterns = load_shiotsuchiignore(&vault_dir);
    let mut all_patterns: Vec<String> = Vec::new();

    // Track unique pattern sources
    all_patterns.extend(config_patterns.iter().cloned());
    all_patterns.extend(ignore_patterns.iter().cloned());

    let check_path = args.path.trim_matches('/');

    match check_ignore(check_path, &all_patterns) {
        Ok(()) => {
            println!("  ✓ NOT excluded: {}", check_path);
            Ok(())
        }
        Err(pattern) => {
            // Find which source
            let source = if ignore_patterns.contains(&pattern) {
                format!(".shiotsuchiignore (pattern: {})", pattern)
            } else if config_patterns.contains(&pattern) {
                format!("exclude_dirs in config.toml (pattern: {})", pattern)
            } else {
                format!("unknown source (pattern: {})", pattern)
            };
            println!("  ✗ EXCLUDED: {}", check_path);
            println!("    Reason: matched {}", source);
            Ok(())
        }
    }
}
