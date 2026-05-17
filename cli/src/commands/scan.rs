use clap::Args;
use shiotsuchi_core::{
    db::NoteDatabase,
    embedder::{resolve_model_path, Embedder},
    models::IndexConfig,
    tokenizer::get_tokenizer,
    watcher::VaultWatcher,
};
use std::{
    path::Path,
    sync::{Arc, Mutex},
};

#[derive(Args, Debug)]
pub struct ScanArgs {
    /// Deprecated: debounce is now managed internally by the file watcher.
    #[arg(long, hide = true)]
    pub debounce: Option<u64>,
}

use crate::config::WatcherConfig;

pub fn run_scan(
    args: &ScanArgs,
    notes_dir: &Path,
    db_path: &Path,
    _watcher_cfg: &WatcherConfig,
    indexing_cfg: &crate::config::IndexingConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(_d) = args.debounce {
        eprintln!("warning: --debounce is deprecated and has no effect; debounce is managed internally by the watcher");
    }
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    crate::util::secure_parent_dir(db_path);

    let embedder = resolve_model_path(None).and_then(|p| match Embedder::load(&p) {
        Ok(e) => {
            eprintln!("[info] Embedder model loaded — vector indexing enabled.");
            Some(e)
        }
        Err(e) => {
            eprintln!("[warn] Could not load embedder: {}.", e);
            None
        }
    });

    if embedder.is_none() {
        eprintln!(
            "[info] Embedder model not found — vector indexing skipped. \
             Run `shiotsuchi setup` to enable semantic search."
        );
    }

    let db = Arc::new(Mutex::new(NoteDatabase::open(db_path)?));
    let tokenizer = get_tokenizer()?;
    let config = IndexConfig {
        vaults: vec![("default".to_string(), notes_dir.to_path_buf())],
        include_extensions: indexing_cfg.include_extensions.clone(),
        exclude_dirs: indexing_cfg.exclude_dirs.clone(),
        auto_exclude_hidden: indexing_cfg.auto_exclude_hidden,
        follow_links: indexing_cfg.follow_links,
        dynamic_threshold: indexing_cfg.dynamic_threshold,
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
