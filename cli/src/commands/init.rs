use crate::config::ShiotsuchiConfig;
use clap::Args;
use std::path::Path;

#[derive(Args, Debug)]
pub struct InitArgs {
    #[arg(long, help = "Overwrite existing config file")]
    pub force: bool,
}

pub fn run_init(
    args: &InitArgs,
    cfg: &ShiotsuchiConfig,
    config_path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    if config_path.exists() && !args.force {
        return Err(format!(
            "Config file already exists at {}. Use --force to overwrite.",
            config_path.display()
        )
        .into());
    }

    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let toml = toml::to_string_pretty(cfg)?;
    std::fs::write(config_path, toml)?;

    println!("Created config file at {}", config_path.display());
    println!("Next, run `shiotsuchi chart` to index your vault.");

    Ok(())
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
        let args = InitArgs { force: false };

        run_init(&args, &cfg, &config_path).unwrap();

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
        let args = InitArgs { force: false };

        let result = run_init(&args, &cfg, &config_path);
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
        let args = InitArgs { force: true };

        run_init(&args, &cfg, &config_path).unwrap();

        let contents = fs::read_to_string(&config_path).unwrap();
        assert!(contents.contains("[vault]"));
    }
}
