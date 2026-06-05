use crate::commands::noise::{scan_vault, ExclusionCandidate, CANDIDATE_LIMIT};
use crate::messages;
use crate::msg_fmt;
use clap::{Args, Subcommand};
use std::path::PathBuf;

#[derive(Args, Debug)]
#[command(about = crate::messages::CONFIG_ABOUT)]
pub struct ConfigArgs {
    #[command(subcommand)]
    pub command: ConfigCommands,
}

#[derive(Subcommand, Debug)]
pub enum ConfigCommands {
    /// ボールト内の除外候補を再検出する
    DetectNoise(DetectNoiseArgs),
    /// Set a config value
    Set(SetArgs),
    /// Reset the embedding API usage counter
    ResetUsage,
}

#[derive(Args, Debug)]
pub struct SetArgs {
    /// Config key (e.g., embedding_usage.enabled)
    pub key: String,
    /// Value to set
    pub value: String,
}

#[derive(Args, Debug)]
pub struct DetectNoiseArgs {
    #[arg(long, help = messages::CONFIG_NOTES_DIR_HELP)]
    pub notes_dir: Option<PathBuf>,
}

fn print_noise_candidates(candidates: &[ExclusionCandidate], label: &str) {
    if candidates.is_empty() {
        println!("{}", msg_fmt!(messages::CONFIG_NO_CANDIDATES, label));
        return;
    }

    println!("{}", msg_fmt!(messages::CONFIG_CANDIDATES_HEADER, label));
    for (i, candidate) in candidates.iter().enumerate() {
        let l = if candidate.is_known_pattern {
            "known"
        } else {
            "dynamic"
        };
        println!("{}", msg_fmt!(messages::CONFIG_CANDIDATE_ITEM, i + 1, candidate.relative_path, l, candidate.file_count, "ル"));
    }
}

fn run_set(args: &SetArgs, config_path: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
    let content = std::fs::read_to_string(config_path)?;
    let mut cfg: toml::Value = toml::from_str(&content)?;

    let parts: Vec<&str> = args.key.split('.').collect();
    match parts.as_slice() {
        ["embedding_usage", "enabled"] => {
            let val: bool = args.value.parse()
                .map_err(|_| format!("Expected bool (true/false), got '{}'", args.value))?;
            cfg["embedding_usage"]["enabled"] = toml::Value::Boolean(val);
        }
        ["embedding_usage", "monthly_limit"] => {
            let val: u64 = args.value.parse()
                .map_err(|_| format!("Expected number, got '{}'", args.value))?;
            cfg["embedding_usage"]["monthly_limit"] = toml::Value::Integer(val as i64);
        }
        _ => return Err(format!("Unknown config key: {}", args.key).into()),
    }

    let toml_str = toml::to_string_pretty(&cfg)?;
    let tmp = config_path.with_extension("toml.tmp");
    std::fs::write(&tmp, &toml_str)?;
    std::fs::rename(&tmp, config_path)?;
    println!("Set {} = {}", args.key, args.value);
    Ok(())
}

fn run_reset_usage(config_dir: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
    let tracker = shiotsuchi_core::usage_tracker::UsageTracker::new(config_dir, true, None);
    tracker.reset()?;
    println!("Embedding API usage counter has been reset.");
    Ok(())
}

pub fn run_config(
    args: &ConfigArgs,
    vaults: &[(String, PathBuf)],
    include_extensions: &[String],
    auto_exclude_hidden: bool,
    dynamic_threshold: usize,
    config_path: &std::path::Path,
    config_dir: &std::path::Path,
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
        ConfigCommands::Set(set_args) => run_set(set_args, config_path)?,
        ConfigCommands::ResetUsage => run_reset_usage(config_dir)?,
    }

    println!();
    println!("{}", messages::CONFIG_RUN_INIT);
    println!("{}", messages::CONFIG_MANUAL_HINT);

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
