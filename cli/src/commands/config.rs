use crate::commands::noise::scan_vault;
use clap::{Args, Subcommand};
use std::path::PathBuf;

#[derive(Args, Debug)]
pub struct ConfigArgs {
    #[command(subcommand)]
    pub command: ConfigCommands,
}

#[derive(Subcommand, Debug)]
pub enum ConfigCommands {
    /// Re-detect exclusion candidates in the vault.
    DetectNoise(DetectNoiseArgs),
}

#[derive(Args, Debug)]
pub struct DetectNoiseArgs {
    /// Vault root to scan (defaults to config's notes_dir).
    #[arg(long)]
    pub notes_dir: Option<PathBuf>,
}

pub fn run_config(
    _args: &ConfigArgs,
    notes_dir: &std::path::Path,
    include_extensions: &[String],
) -> Result<(), Box<dyn std::error::Error>> {
    // For now, detect-noise is the only subcommand.
    let candidates = scan_vault(notes_dir, include_extensions);

    if candidates.is_empty() {
        println!("No exclusion candidates detected in {}", notes_dir.display());
        return Ok(());
    }

    println!("Exclusion candidates in {}:", notes_dir.display());
    println!();
    for (i, candidate) in candidates.iter().enumerate() {
        let label = if candidate.is_known_pattern {
            "known"
        } else {
            "dynamic"
        };
        println!(
            "  {}. {} [{}] ({} file{})",
            i + 1,
            candidate.relative_path,
            label,
            candidate.file_count,
            if candidate.file_count == 1 { "" } else { "s" }
        );
    }
    println!();
    println!("Run `shiotsuchi init --force` to regenerate config with these exclusions.");
    println!("Or add them manually to the [indexing] section of your config file.");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_detect_noise_empty_vault() {
        let temp = TempDir::new().unwrap();
        let vault = temp.path().join("vault");
        fs::create_dir(&vault).unwrap();

        let candidates = scan_vault(&vault, &["md".to_string(), "markdown".to_string()]);
        assert!(candidates.is_empty());
    }

    #[test]
    fn test_detect_noise_detects_candidates() {
        let temp = TempDir::new().unwrap();
        let vault = temp.path().join("vault");
        fs::create_dir(&vault).unwrap();

        // Create noise dirs
        for dir in &["node_modules", "dist", "build"] {
            let d = vault.join(dir);
            fs::create_dir(&d).unwrap();
            fs::write(d.join("f.md"), "# test").unwrap();
        }

        let candidates = scan_vault(&vault, &["md".to_string(), "markdown".to_string()]);
        assert_eq!(candidates.len(), 3);

        let paths: Vec<_> = candidates.iter().map(|c| c.relative_path.as_str()).collect();
        assert!(paths.contains(&"node_modules"));
        assert!(paths.contains(&"dist"));
        assert!(paths.contains(&"build"));
    }
}
