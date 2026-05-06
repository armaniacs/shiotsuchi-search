use clap::Args;
use shiotsuchi_core::{
    db::NoteDatabase,
    models::IndexConfig,
    tokenizer::get_tokenizer,
    watcher::VaultWatcher,
};
use std::{path::Path, sync::{Arc, Mutex}};

#[derive(Args, Debug)]
pub struct ScanArgs {
    #[arg(long, default_value = "500")]
    pub debounce: u64,
}

use crate::config::WatcherConfig;

pub fn run_scan(
    _args: &ScanArgs,
    notes_dir: &Path,
    db_path: &Path,
    _watcher_cfg: &WatcherConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let db = Arc::new(Mutex::new(NoteDatabase::open(db_path)?));
    let tokenizer = get_tokenizer()?;
    let config = IndexConfig {
        notes_dir: notes_dir.to_path_buf(),
        ..Default::default()
    };
    let watcher = VaultWatcher::new(db, tokenizer, config);
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
            notes_dir: vault,
            ..Default::default()
        };
        let _watcher = VaultWatcher::new(db, tokenizer, config);
    }
}