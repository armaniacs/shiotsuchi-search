use crate::commands::noise::{scan_vault, ExclusionCandidate, CANDIDATE_LIMIT};
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
    /// Vault root to scan (defaults to all configured vaults).
    #[arg(long)]
    pub notes_dir: Option<PathBuf>,
}

fn print_noise_candidates(candidates: &[ExclusionCandidate], label: &str) {
    if candidates.is_empty() {
        println!("No exclusion candidates detected in {}", label);
        return;
    }

    println!("Exclusion candidates in {}:", label);
    for (i, candidate) in candidates.iter().enumerate() {
        let l = if candidate.is_known_pattern {
            "known"
        } else {
            "dynamic"
        };
        println!(
            "  {}. {} [{}] ({} file{})",
            i + 1,
            candidate.relative_path,
            l,
            candidate.file_count,
            if candidate.file_count == 1 { "" } else { "s" }
        );
    }
}

pub fn run_config(
    args: &ConfigArgs,
    vaults: &[(String, PathBuf)],
    include_extensions: &[String],
    auto_exclude_hidden: bool,
    dynamic_threshold: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    match &args.command {
        ConfigCommands::DetectNoise(detect_args) => {
            if let Some(custom_dir) = &detect_args.notes_dir {
                let (candidates, _truncated) = scan_vault(
                    custom_dir,
                    include_extensions,
                    auto_exclude_hidden,
                    dynamic_threshold,
                    CANDIDATE_LIMIT,
                );
                print_noise_candidates(&candidates, &custom_dir.display().to_string());
            } else {
                for (name, vault_dir) in vaults {
                    let (candidates, _truncated) = scan_vault(
                        vault_dir,
                        include_extensions,
                        auto_exclude_hidden,
                        dynamic_threshold,
                        CANDIDATE_LIMIT,
                    );
                    print_noise_candidates(&candidates, name);
                }
            }
        }
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

        let (candidates, _truncated) = scan_vault(
            &vault,
            &["md".to_string(), "markdown".to_string()],
            true,
            5,
            1000,
        );
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

        let (candidates, _truncated) = scan_vault(
            &vault,
            &["md".to_string(), "markdown".to_string()],
            true,
            5,
            1000,
        );
        assert_eq!(candidates.len(), 3);

        let paths: Vec<_> = candidates
            .iter()
            .map(|c| c.relative_path.as_str())
            .collect();
        assert!(paths.contains(&"node_modules"));
        assert!(paths.contains(&"dist"));
        assert!(paths.contains(&"build"));
    }
}
