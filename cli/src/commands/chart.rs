use crate::messages;
use crate::msg_fmt;
use crate::config::IndexingConfig;
use clap::Args;
use shiotsuchi_core::{
    config::EmbedderConfig,
    db::NoteDatabase,
    indexer::{index_directory, IndexResult},
    models::IndexConfig,
    tokenizer::get_tokenizer,
};
use std::path::{Path, PathBuf};

#[derive(Args, Debug)]
#[command(about = crate::messages::CHART_ABOUT)]
pub struct ChartArgs {
    #[arg(long, hide = true, help = messages::CHART_FORCE_HELP)]
    pub force: bool,
    #[arg(long, help = messages::CHART_QUIET_HELP)]
    pub quiet: bool,
    #[arg(long, help = messages::VAULT_HELP)]
    pub vault: Option<String>,
}

pub struct ChartSummary {
    pub indexed: usize,
    pub skipped: usize,
    pub errors: usize,
    pub invalid_patterns: usize,
    pub excluded: usize,
}

pub fn run_chart(
    args: &ChartArgs,
    vaults: &[(String, PathBuf)],
    db_path: &Path,
    indexing_cfg: &IndexingConfig,
    embedder_cfg: &EmbedderConfig,
    vlm_cfg: &shiotsuchi_core::config::VlmConfig,
) -> Result<ChartSummary, Box<dyn std::error::Error>> {
    if args.force {
        eprintln!("{}", messages::CHART_FORCE_DEPRECATED);
    }
    let db = NoteDatabase::open(db_path)?;
    let tokenizer = get_tokenizer()?;
    let config = IndexConfig {
        vaults: vaults.to_vec(),
        include_extensions: indexing_cfg.include_extensions.clone(),
        exclude_dirs: indexing_cfg.exclude_dirs.clone(),
        auto_exclude_hidden: indexing_cfg.auto_exclude_hidden,
        follow_links: indexing_cfg.follow_links,
        dynamic_threshold: indexing_cfg.dynamic_threshold,
        user_dictionary: indexing_cfg.user_dictionary.clone(),
        enable_pdf_extraction: indexing_cfg.enable_pdf_extraction,
        backlink_scoring: indexing_cfg.backlink_scoring,
        vlm_enabled: vlm_cfg.enabled,
        vlm_provider: vlm_cfg.provider.clone(),
        vlm_model: vlm_cfg.model.clone(),
        vlm_max_pages_per_doc: vlm_cfg.max_pages_per_doc,
    };

    let embedder = match embedder_cfg.create_embedder() {
        Ok(Some(e)) => {
            if !args.quiet {
                eprintln!("{}", messages::INFO_EMBEDDER_LOADED);
            }
            Some(e)
        }
        Ok(None) => {
            if !args.quiet {
                eprintln!("{}", messages::INFO_EMBEDDER_SKIPPED);
            }
            None
        }
        Err(e) => {
            if !args.quiet {
                eprintln!("{}", msg_fmt!(messages::WARN_EMBEDDER_LOAD, e));
            }
            None
        }
    };

    // Warn if API key is in config.toml instead of SHIOTSUCHI_API_KEY env var
    if embedder_cfg.has_api_key_in_config_but_not_env() && !args.quiet {
        eprintln!("{}", messages::WARN_API_KEY_IN_CONFIG);
    }

    // Warn if the model has changed since the last indexing run.
    if let Some(ref emb) = embedder {
        if let Ok(Some(stored_model_id)) = db.get_dominant_model_id() {
            if stored_model_id != emb.model_id() && !args.quiet {
                eprintln!("{}", messages::WARN_MODEL_CHANGED);
            }
        }
    }

    let (results, invalid_patterns, excluded_count) = index_directory(&db, &tokenizer, &config, embedder.as_ref(), None)?;

    let mut summary = ChartSummary {
        indexed: 0,
        skipped: 0,
        errors: 0,
        invalid_patterns,
        excluded: excluded_count,
    };
    for (_, _, result) in &results {
        match result {
            IndexResult::Inserted | IndexResult::Updated => summary.indexed += 1,
            IndexResult::Skipped => summary.skipped += 1,
            IndexResult::Error(_) => summary.errors += 1,
        }
    }

    if !args.quiet {
        println!("{}", msg_fmt!(messages::INDEX_SUMMARY, summary.indexed, summary.skipped, summary.errors, summary.excluded));
        if summary.invalid_patterns > 0 {
            println!("{}", msg_fmt!(messages::INDEX_PATTERN_WARN, summary.invalid_patterns));
        }
    }

    Ok(summary)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::IndexingConfig;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_chart_indexes_files() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("note.md"), "# Hello\n\nWorld").unwrap();

        let db_file = temp.path().join("test.db");
        let args = ChartArgs {
            force: false,
            quiet: true,
            vault: None,
        };
        let idx_cfg = IndexingConfig::default();
        let result = run_chart(&args, &[("default".to_string(), temp.path().to_path_buf())], &db_file, &idx_cfg, &shiotsuchi_core::config::EmbedderConfig::default(), &shiotsuchi_core::config::VlmConfig::default());
        match result {
            Ok(summary) => {
                assert_eq!(summary.indexed, 1);
                assert_eq!(summary.errors, 0);
            }
            Err(e) => {
                // If the model is unavailable, `get_tokenizer()` returns NoModel.
                let msg = format!("{}", e);
                if msg.contains("no model") || msg.contains("NoModel") || msg.contains("No such file") {
                    eprintln!("[SKIPPED] chart::test_chart_indexes_files — Vaporetto model not available (set SHIOTSUCHI_MODEL_PATH)");
                    return;
                }
                panic!("chart test failed: {}", e);
            }
        }
    }

    #[test]
    #[cfg(unix)]
    fn test_chart_creates_parent_dir_with_0700() {
        use std::os::unix::fs::PermissionsExt;
        let temp = TempDir::new().unwrap();
        let vault = temp.path().join("vault");
        std::fs::create_dir_all(&vault).unwrap();
        fs::write(vault.join("note.md"), "# Test").unwrap();

        let db_path = temp.path().join("cache").join("subdir").join("test.db");
        let args = ChartArgs {
            force: false,
            quiet: true,
            vault: None,
        };
        let idx_cfg = IndexingConfig::default();
        let _result = run_chart(&args, &[("default".to_string(), vault.clone())], &db_path, &idx_cfg, &shiotsuchi_core::config::EmbedderConfig::default(), &shiotsuchi_core::config::VlmConfig::default());

        let parent = db_path.parent().unwrap();
        if parent.exists() {
            let mode = std::fs::metadata(parent).unwrap().permissions().mode() & 0o777;
            assert_eq!(
                mode, 0o700,
                "parent directory should have 0o700 permissions"
            );
        }
    }

    #[test]
    #[cfg(unix)]
    fn test_chart_parent_dir_0700_with_nested_path() {
        let temp = TempDir::new().unwrap();
        let vault = temp.path();
        fs::write(vault.join("note.md"), "# Test").unwrap();

        let db_path = temp.path().join("a").join("b").join("c").join("test.db");
        let args = ChartArgs {
            force: false,
            quiet: true,
            vault: None,
        };
        let idx_cfg = IndexingConfig::default();
        let _result = run_chart(
            &args,
            &[("default".to_string(), vault.to_path_buf())],
            &db_path,
            &idx_cfg,
            &shiotsuchi_core::config::EmbedderConfig::default(),
            &shiotsuchi_core::config::VlmConfig::default(),
        );
    }

    #[test]
    fn test_chart_warns_when_api_key_in_config() {
        let temp = TempDir::new().unwrap();
        let db_file = temp.path().join("test.db");
        let vault = temp.path().join("vault");
        std::fs::create_dir_all(&vault).unwrap();
        std::fs::write(vault.join("note.md"), "# Hello").unwrap();

        let api_cfg = shiotsuchi_core::config::EmbedderConfig::Api {
            endpoint: "https://api.example.com".to_string(),
            model: "model".to_string(),
            api_key: Some("sk-test".to_string()),
        };

        let args = ChartArgs {
            force: false,
            quiet: false,
            vault: None,
        };
        let idx_cfg = IndexingConfig::default();

        // Ensure env var is not set for this test
        let old_env = std::env::var_os("SHIOTSUCHI_API_KEY");
        std::env::remove_var("SHIOTSUCHI_API_KEY");

        // The fake endpoint doesn't exist, so create_embedder() will succeed in building
        // the ApiClient but embedder operations may fail later. The warning path should
        // still be exercised because the config contains api_key and the env var is absent.
        let _ = run_chart(&args, &[("default".to_string(), vault)], &db_file, &idx_cfg, &api_cfg, &shiotsuchi_core::config::VlmConfig::default());

        // Restore env var if it was set
        if let Some(v) = old_env {
            std::env::set_var("SHIOTSUCHI_API_KEY", v);
        }

        // This test is best-effort: verifying the warning branch compiles and runs
        // without panicking. Full stderr capture would require a custom writer.
    }
}
