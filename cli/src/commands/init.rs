use crate::commands::noise::{scan_vault, ExclusionCandidate, CANDIDATE_LIMIT};
use crate::config::ShiotsuchiConfig;
use clap::Args;
use dialoguer::{theme::ColorfulTheme, Confirm, MultiSelect};
use std::path::{Path, PathBuf};

#[derive(Args, Debug)]
pub struct InitArgs {
    #[arg(long, help = "Overwrite existing config file")]
    pub force: bool,
    #[arg(
        long,
        help = "Non-interactive mode: auto-accept all detected exclusion candidates"
    )]
    pub yes: bool,
}

pub fn run_init(
    args: &InitArgs,
    cfg: &ShiotsuchiConfig,
    config_path: &Path,
    raw_notes_dir: Option<&Path>,
    raw_db_path: Option<&Path>,
) -> Result<(), Box<dyn std::error::Error>> {
    // --- Backup existing config if --force ---
    if config_path.exists() {
        if !args.force {
            return Err(format!(
                "Config file already exists at {}. Use --force to overwrite.",
                config_path.display()
            )
            .into());
        }
        backup_config(config_path)?;
    }

    // --- Determine effective notes_dir ---
    let effective_notes_dir: PathBuf = match raw_notes_dir {
        Some(dir) => dir.to_path_buf(),
        None => {
            // When --force is used, preserve the existing notes_dir if it was
            // explicitly set (not the default ".").
            if args.force && cfg.vault.notes_dir != std::path::Path::new(".") {
                cfg.vault.notes_dir.clone()
            } else {
                let cwd = std::env::current_dir()?;
                eprintln!(
                    "info: --notes-dir not specified, scanning current directory: {}",
                    cwd.display()
                );
                eprintln!("info: use --notes-dir <PATH> to scan a different vault root.");
                cwd
            }
        }
    };

    // --- Create config directory if needed ---
    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // --- Build the output config ---
    let mut out_cfg = cfg.clone();
    out_cfg.vault.notes_dir = effective_notes_dir.clone();
    if let Some(db) = raw_db_path {
        out_cfg.vault.db_path = db.to_path_buf();
    }

    // --- Validate and scan vault ---
    if !effective_notes_dir.exists() {
        return Err(format!(
            "Notes directory does not exist: {}",
            effective_notes_dir.display()
        )
        .into());
    }
    let (candidates, _truncated) = scan_vault(
        &effective_notes_dir,
        &out_cfg.indexing.include_extensions,
        out_cfg.indexing.auto_exclude_hidden,
        out_cfg.indexing.dynamic_threshold,
        CANDIDATE_LIMIT,
    );

    // --- Interactive or non-interactive exclusion selection ---
    let is_tty = dialoguer_stdin_is_tty();
    let selected_patterns: Vec<String> = if candidates.is_empty() {
        Vec::new()
    } else if is_tty && !args.yes {
        // TTY mode: 2-stage interactive UI
        select_exclusions_interactive(&candidates)?
    } else if !is_tty && !args.yes {
        // Non-TTY without --yes: require explicit opt-in when candidates exist.
        return Err(
            "Interactive mode requires a TTY. Use --yes to auto-accept all exclusion candidates, \
             or run in a terminal."
                .into(),
        );
    } else {
        // Non-TTY with --yes, or TTY with --yes: auto-accept all candidates.
        eprintln!(
            "info: auto-accepting {} exclusion candidate(s).",
            candidates.len()
        );
        candidates.iter().map(|c| c.relative_path.clone()).collect()
    };

    // Merge selected patterns with existing exclude_dirs so that
    // manually added custom patterns are not lost on --force.
    let mut merged: Vec<String> = cfg.indexing.exclude_dirs.clone();
    for p in selected_patterns {
        if !merged.contains(&p) {
            merged.push(p);
        }
    }
    out_cfg.indexing.exclude_dirs = merged;

    // --- Write config atomically with restricted permissions ---
    let toml = toml::to_string_pretty(&out_cfg)?;
    let tmp_path = config_path.with_extension("toml.tmp");
    std::fs::write(&tmp_path, toml)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp_path, std::fs::Permissions::from_mode(0o600))?;
    }
    std::fs::rename(&tmp_path, config_path)?;

    println!("Created config file at {}", config_path.display());
    if !candidates.is_empty() {
        println!(
            "Excluded {} director{} from indexing.",
            out_cfg.indexing.exclude_dirs.len(),
            if out_cfg.indexing.exclude_dirs.len() == 1 {
                "y"
            } else {
                "ies"
            }
        );
    }
    println!("Next, run `shiotsuchi chart` to index your vault.");

    Ok(())
}

/// Create a timestamped backup of the existing config file.
/// Uses Unix epoch seconds + microseconds to ensure unique, sortable timestamps.
fn backup_config(config_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = now.as_secs();
    let micros = now.subsec_micros();
    let timestamp = format!("{}.{:06}", secs, micros);
    let mut backup_path = config_path.with_extension(format!("toml.bak.{}", timestamp));
    // Avoid overwriting an existing backup (e.g. fast successive runs).
    let mut counter = 1u32;
    while backup_path.exists() {
        backup_path = config_path.with_extension(format!("toml.bak.{}.{}", timestamp, counter));
        counter += 1;
    }
    std::fs::copy(config_path, &backup_path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Err(e) =
            std::fs::set_permissions(&backup_path, std::fs::Permissions::from_mode(0o600))
        {
            let _ = std::fs::remove_file(&backup_path);
            return Err(e.into());
        }
    }
    println!("Backed up existing config to {}", backup_path.display());
    Ok(())
}

/// 2-stage interactive exclusion UI.
///
/// Stage 1: "Exclude common build/output directories?" (Confirm)
///   - Yes: all known-pattern candidates are pre-selected in Stage 2.
///   - No:  known-pattern candidates start unselected.
///
/// Stage 2: Multi-select showing ALL candidates. Dynamic (non-known)
/// candidates are always pre-selected. Known-pattern pre-selection
/// depends on Stage 1 choice.
fn select_exclusions_interactive(
    candidates: &[ExclusionCandidate],
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    if candidates.is_empty() {
        return Ok(Vec::new());
    }

    // Separate into known-pattern and dynamic candidates.
    let known_indices: Vec<usize> = candidates
        .iter()
        .enumerate()
        .filter(|(_, c)| c.is_known_pattern)
        .map(|(i, _)| i)
        .collect();
    let _dynamic_indices: Vec<usize> = candidates
        .iter()
        .enumerate()
        .filter(|(_, c)| !c.is_known_pattern)
        .map(|(i, _)| i)
        .collect();

    // Stage 1: bulk confirm known patterns.
    let bulk_exclude_known = if !known_indices.is_empty() {
        Confirm::with_theme(&ColorfulTheme::default())
            .with_prompt("Exclude common build/output directories?")
            .default(true)
            .interact()?
    } else {
        true // no known patterns, skip stage 1
    };

    // Determine pre-selected items for Stage 2.
    let mut defaults: Vec<bool> = vec![false; candidates.len()];
    // Known-pattern candidates are pre-selected by default; dynamic candidates
    // start unselected so the user must explicitly choose them.
    if bulk_exclude_known {
        for &i in &known_indices {
            defaults[i] = true;
        }
    }

    // Build display strings: "dirname (N files)"
    let display_items: Vec<String> = candidates
        .iter()
        .map(|c| {
            format!(
                "{} ({} file{})",
                c.relative_path,
                c.file_count,
                if c.file_count == 1 { "" } else { "s" }
            )
        })
        .collect();

    // Stage 2: multi-select.
    let selections = MultiSelect::with_theme(&ColorfulTheme::default())
        .items(&display_items)
        .defaults(&defaults)
        .interact()?;

    let selected: Vec<String> = selections
        .iter()
        .map(|&i| candidates[i].relative_path.clone())
        .collect();

    Ok(selected)
}

/// Check whether stdin and stdout are both TTYs (interactive terminal).
fn dialoguer_stdin_is_tty() -> bool {
    use std::io::IsTerminal;
    std::io::stdin().is_terminal() && std::io::stdout().is_terminal()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ShiotsuchiConfig;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_init_creates_config() {
        let temp = TempDir::new().unwrap();
        let config_path = temp.path().join("config.toml");
        let cfg = ShiotsuchiConfig::default();
        let args = InitArgs {
            force: false,
            yes: true,
        };

        run_init(&args, &cfg, &config_path, None, None).unwrap();

        assert!(config_path.exists());
        let contents = fs::read_to_string(&config_path).unwrap();
        assert!(contents.contains("[vault]"));
        assert!(contents.contains("[indexing]"));
        assert!(contents.contains("[watcher]"));
    }

    #[test]
    fn test_init_refuses_overwrite_without_force() {
        let temp = TempDir::new().unwrap();
        let config_path = temp.path().join("config.toml");
        fs::write(&config_path, "existing").unwrap();

        let cfg = ShiotsuchiConfig::default();
        let args = InitArgs {
            force: false,
            yes: true,
        };

        let result = run_init(&args, &cfg, &config_path, None, None);
        assert!(result.is_err());

        let contents = fs::read_to_string(&config_path).unwrap();
        assert_eq!(contents, "existing");
    }

    #[test]
    fn test_init_overwrites_with_force() {
        let temp = TempDir::new().unwrap();
        let config_path = temp.path().join("config.toml");
        fs::write(&config_path, "existing").unwrap();

        let cfg = ShiotsuchiConfig::default();
        let args = InitArgs {
            force: true,
            yes: true,
        };

        run_init(&args, &cfg, &config_path, None, None).unwrap();

        let contents = fs::read_to_string(&config_path).unwrap();
        assert!(contents.contains("[vault]"));
        // The original file should be replaced
        assert!(contents.contains("[indexing]"));
    }

    #[test]
    fn test_init_creates_timestamped_backup() {
        let temp = TempDir::new().unwrap();
        let config_path = temp.path().join("config.toml");
        fs::write(&config_path, "original content").unwrap();

        let cfg = ShiotsuchiConfig::default();
        let args = InitArgs {
            force: true,
            yes: true,
        };

        run_init(&args, &cfg, &config_path, None, None).unwrap();

        // A backup file should exist with the .toml.bak.YYYYMMDD-* extension
        let parent = config_path.parent().unwrap();
        let entries: Vec<_> = fs::read_dir(parent)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().contains("config.toml.bak."))
            .collect();
        assert_eq!(entries.len(), 1, "Should have exactly one backup file");

        // Verify backup content is the original
        let backup_content = fs::read_to_string(&entries[0].path()).unwrap();
        assert_eq!(backup_content, "original content");
    }

    #[test]
    fn test_init_detects_exclusion_candidates() {
        let temp = TempDir::new().unwrap();
        let vault = temp.path().join("vault");
        fs::create_dir(&vault).unwrap();

        // Create noise directories
        for dir in &["node_modules", "dist", "templates"] {
            let d = vault.join(dir);
            fs::create_dir(&d).unwrap();
            fs::write(d.join("file.md"), "# content").unwrap();
        }

        let config_path = temp.path().join("config.toml");
        let cfg = ShiotsuchiConfig::default();
        let args = InitArgs {
            force: false,
            yes: true,
        };

        run_init(&args, &cfg, &config_path, Some(&vault), None).unwrap();

        let contents = fs::read_to_string(&config_path).unwrap();
        // The exclude_dirs should include our noise dirs
        assert!(contents.contains("node_modules"));
        assert!(contents.contains("dist"));
        assert!(contents.contains("templates"));
    }

    #[test]
    fn test_init_creates_config_without_candidates() {
        let temp = TempDir::new().unwrap();
        let vault = temp.path().join("vault");
        fs::create_dir(&vault).unwrap();

        // Empty vault — no candidates
        let config_path = temp.path().join("config.toml");
        let cfg = ShiotsuchiConfig::default();
        let args = InitArgs {
            force: false,
            yes: true,
        };

        run_init(&args, &cfg, &config_path, Some(&vault), None).unwrap();

        let contents = fs::read_to_string(&config_path).unwrap();
        // Default exclude_dirs (node_modules) is preserved when no candidates are found.
        assert!(contents.contains(r#"exclude_dirs = ["node_modules"]"#));
    }

    #[test]
    fn test_backup_config() {
        let temp = TempDir::new().unwrap();
        let config_path = temp.path().join("config.toml");
        fs::write(&config_path, "test data").unwrap();

        backup_config(&config_path).unwrap();

        // Backup file created
        let parent = config_path.parent().unwrap();
        let backup_files: Vec<_> = fs::read_dir(parent)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().contains("config.toml.bak."))
            .collect();
        assert_eq!(backup_files.len(), 1);

        // Content preserved
        let content = fs::read_to_string(&backup_files[0].path()).unwrap();
        assert_eq!(content, "test data");
    }

    #[test]
    fn test_init_with_notes_dir_override() {
        let temp = TempDir::new().unwrap();
        let vault = temp.path().join("my_vault");
        fs::create_dir(&vault).unwrap();
        fs::write(vault.join("note.md"), "# My Note").unwrap();

        let config_path = temp.path().join("config.toml");
        let cfg = ShiotsuchiConfig::default();
        let args = InitArgs {
            force: false,
            yes: true,
        };

        run_init(&args, &cfg, &config_path, Some(&vault), None).unwrap();

        let contents = fs::read_to_string(&config_path).unwrap();
        let vault_str = vault.to_string_lossy().to_string();
        assert!(contents.contains(&vault_str));
    }

    #[test]
    fn test_init_preserves_existing_exclude_dirs() {
        let temp = TempDir::new().unwrap();
        let vault = temp.path().join("vault");
        fs::create_dir(&vault).unwrap();

        // Create a noise directory that scan_vault will detect
        let node_modules = vault.join("node_modules");
        fs::create_dir(&node_modules).unwrap();
        fs::write(node_modules.join("dep.md"), "# Dep").unwrap();

        // Build a config with pre-existing custom exclude patterns
        let mut cfg = ShiotsuchiConfig::default();
        cfg.indexing.exclude_dirs = vec!["legacy".to_string(), "private".to_string()];

        let config_path = temp.path().join("config.toml");
        let args = InitArgs {
            force: true,
            yes: true,
        };

        run_init(&args, &cfg, &config_path, Some(&vault), None).unwrap();

        let contents = fs::read_to_string(&config_path).unwrap();
        assert!(
            contents.contains("node_modules"),
            "scan result should be merged"
        );
        assert!(
            contents.contains("legacy"),
            "existing custom pattern should be preserved"
        );
        assert!(
            contents.contains("private"),
            "existing custom pattern should be preserved"
        );
    }

    #[test]
    #[cfg(unix)]
    fn test_config_file_permissions_0600() {
        let temp = TempDir::new().unwrap();
        let vault = temp.path().join("vault");
        fs::create_dir(&vault).unwrap();

        let config_path = temp.path().join("config.toml");
        let cfg = ShiotsuchiConfig::default();
        let args = InitArgs {
            force: false,
            yes: true,
        };

        run_init(&args, &cfg, &config_path, Some(&vault), None).unwrap();

        use std::os::unix::fs::PermissionsExt;
        let metadata = fs::metadata(&config_path).unwrap();
        let permissions = metadata.permissions();
        let mode = permissions.mode();
        assert_eq!(
            mode & 0o777,
            0o600,
            "config file should have mode 0o600, got {:o}",
            mode & 0o777
        );
    }

    #[test]
    #[cfg(unix)]
    fn test_backup_file_permissions_0600() {
        let temp = TempDir::new().unwrap();
        let vault = temp.path().join("vault");
        fs::create_dir(&vault).unwrap();

        let config_path = temp.path().join("config.toml");
        fs::write(&config_path, "existing config").unwrap();

        let cfg = ShiotsuchiConfig::default();
        let args = InitArgs {
            force: true,
            yes: true,
        };

        run_init(&args, &cfg, &config_path, Some(&vault), None).unwrap();

        let parent = config_path.parent().unwrap();
        let backup_files: Vec<_> = fs::read_dir(parent)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| {
                let name = e.file_name().to_string_lossy().to_string();
                name.contains(".toml.bak.")
            })
            .collect();

        assert_eq!(backup_files.len(), 1, "should have exactly one backup file");

        use std::os::unix::fs::PermissionsExt;
        let metadata = fs::metadata(&backup_files[0].path()).unwrap();
        let permissions = metadata.permissions();
        let mode = permissions.mode();
        assert_eq!(
            mode & 0o777,
            0o600,
            "backup file should have mode 0o600, got {:o}",
            mode & 0o777
        );
    }
}
