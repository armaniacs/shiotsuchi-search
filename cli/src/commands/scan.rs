use crate::messages;
use crate::msg_fmt;
use clap::Args;
use shiotsuchi_core::{
    config::EmbedderConfig,
    db::NoteDatabase,
    models::IndexConfig,
    tokenizer::get_tokenizer,
    watcher::VaultWatcher,
};
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

#[derive(Args, Debug)]
#[command(about = crate::messages::SCAN_ABOUT)]
pub struct ScanArgs {
    #[arg(long, hide = true, help = messages::SCAN_DEBOUNCE_HELP)]
    pub debounce: Option<u64>,
    #[arg(long, help = messages::VAULT_HELP)]
    pub vault: Option<String>,
}

use crate::config::WatcherConfig;

pub fn run_scan(
    args: &ScanArgs,
    vaults: &[(String, PathBuf)],
    db_path: &Path,
    _watcher_cfg: &WatcherConfig,
    indexing_cfg: &crate::config::IndexingConfig,
    embedder_cfg: &EmbedderConfig,
    vlm_cfg: &shiotsuchi_core::config::VlmConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(_d) = args.debounce {
        eprintln!("{}", messages::SCAN_DEBOUNCE_DEPRECATED);
    }

    // Warn if API key is in config.toml instead of SHIOTSUCHI_API_KEY env var
    if embedder_cfg.has_api_key_in_config_but_not_env() {
        eprintln!("{}", messages::WARN_API_KEY_IN_CONFIG);
    }

    let embedder = match embedder_cfg.create_embedder(&indexing_cfg.embedding_usage) {
        Ok(Some(e)) => {
            eprintln!("{}", messages::INFO_EMBEDDER_LOADED);
            Some(e)
        }
        Ok(None) => {
            eprintln!("{}", messages::INFO_EMBEDDER_SKIPPED);
            None
        }
        Err(e) => {
            eprintln!("{}", msg_fmt!(messages::WARN_EMBEDDER_LOAD, e));
            None
        }
    };

    let db = Arc::new(Mutex::new(NoteDatabase::open(db_path)?));

    // Warn if the model has changed since the last indexing run.
    if let Some(ref emb) = embedder {
        let guard = db.lock().unwrap();
        if let Ok(Some(stored_model_id)) = guard.get_dominant_model_id() {
            if stored_model_id != emb.model_id() {
                eprintln!("{}", messages::WARN_MODEL_CHANGED);
            }
        }
    }

    // VLM consent check: scan is non-interactive, so just log a warning
    if vlm_cfg.enabled && !vlm_cfg.consent_obtained {
        eprintln!(
            "[warn] VLM is enabled but consent has not been obtained. \
             Run `shiotsuchi chart` interactively to grant consent, or set \
             `vlm.consent_obtained = true` in config.toml."
        );
    }

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
        vlm_consent_obtained: vlm_cfg.consent_obtained,
        vlm_provider: vlm_cfg.provider.clone(),
        vlm_model: vlm_cfg.model.clone(),
        vlm_max_pages_per_doc: vlm_cfg.max_pages_per_doc,
        embedding_usage: indexing_cfg.embedding_usage.clone(),
    };
    let watcher = VaultWatcher::new(db, tokenizer, config, embedder);
    watcher.watch()
}

#[cfg(test)]
mod tests {
    use super::*;
    use shiotsuchi_core::tokenizer::{JapaneseTokenizer, TokenizerConfig};
    use tempfile::TempDir;

    /// Tests that the watcher can be constructed from CLI configuration.
    /// The event handling logic is tested in core/src/watcher.rs tests.
    /// This replaces the previous flaky PollWatcher + real-sleep approach
    /// that caused 60s+ timeouts on some systems.
    #[test]
    fn test_scan_watcher_setup() {
        let tokenizer = match JapaneseTokenizer::new(TokenizerConfig::default()) {
            Ok(tok) => Arc::new(tok),
            Err(_) => return,
        };
        let temp = TempDir::new().unwrap();
        let vault = temp.path().join("vault");
        std::fs::create_dir_all(&vault).unwrap();
        let db_file = temp.path().join("test.db");
        let db = Arc::new(Mutex::new(NoteDatabase::open(&db_file).unwrap()));
        let config = IndexConfig {
            vaults: vec![("default".to_string(), vault)],
            ..Default::default()
        };
        let _watcher = VaultWatcher::new(db, tokenizer, config, None);
    }

    #[test]
    #[cfg(unix)]
    fn test_scan_parent_dir_0700_via_utility() {
        use std::os::unix::fs::PermissionsExt;
        let temp = TempDir::new().unwrap();
        let db_path = temp.path().join("cache").join("scan-test.db");
        let db_dir = db_path.parent().unwrap();
        std::fs::create_dir_all(db_dir).unwrap();

        // Same path that run_scan uses
        crate::util::secure_parent_dir(&db_path);

        let mode = std::fs::metadata(db_dir).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o700, "scan parent dir should have 0o700 permissions");
    }
}
