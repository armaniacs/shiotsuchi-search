use crate::config::ShiotsuchiConfig;
use crate::commands::noise::{scan_vault, ExclusionCandidate};
use clap::Args;
use dialoguer::{theme::ColorfulTheme, Confirm, MultiSelect};
use std::path::{Path, PathBuf};

#[derive(Args, Debug)]
pub struct InitArgs {
    #[arg(long, help = "Overwrite existing config file")]
    pub force: bool,
    #[arg(long, help = "Non-interactive mode: auto-accept all detected exclusion candidates")]
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
            let cwd = std::env::current_dir()?;
            eprintln!(
                "info: --notes-dir not specified, scanning current directory: {}",
                cwd.display()
            );
            eprintln!("info: use --notes-dir <PATH> to scan a different vault root.");
            cwd
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
    let candidates = scan_vault(&effective_notes_dir, &out_cfg.indexing.include_extensions);

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

    out_cfg.indexing.exclude_patterns = selected_patterns;

    // --- Write config ---
    let toml = toml::to_string_pretty(&out_cfg)?;
    std::fs::write(config_path, toml)?;

    println!("Created config file at {}", config_path.display());
    if !candidates.is_empty() {
        println!(
            "Excluded {} director{} from indexing.",
            out_cfg.indexing.exclude_patterns.len(),
            if out_cfg.indexing.exclude_patterns.len() == 1 {
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
/// Uses sub-second precision (%6f = microseconds) to avoid collisions
/// when multiple backups are created within the same second.
fn backup_config(config_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let timestamp = chrono::Local::now().format("%Y%m%d-%H%M%S.%6f");
    let backup_path = config_path.with_extension(format!("toml.bak.{}", timestamp));
    std::fs::copy(config_path, &backup_path)?;
    println!(
        "Backed up existing config to {}",
        backup_path.display()
    );
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
fn select_exclusions_interactive(candidates: &[ExclusionCandidate]) -> Result<Vec<String>, Box<dyn std::error::Error>> {
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
    let dynamic_indices: Vec<usize> = candidates
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
    for &i in &dynamic_indices {
        defaults[i] = true; // dynamic candidates always pre-selected
    }
    if bulk_exclude_known {
        for &i in &known_indices {
            defaults[i] = true; // known candidates pre-selected if user chose Yes
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

/// Check whether stdin is a TTY (interactive terminal).
fn dialoguer_stdin_is_tty() -> bool {
    use std::io::IsTerminal;
    std::io::Stdin::is_terminal(&std::io::stdin())
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
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .contains("config.toml.bak.")
            })
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
        // The exclude_patterns should include our noise dirs
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
        // exclude_patterns should be empty
        assert!(contents.contains("exclude_patterns = []"));
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
}
