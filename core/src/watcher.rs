use crate::{
    db::NoteDatabase,
    embedder::Embedder,
    indexer::{index_file_with_embedder, IndexResult},
    models::IndexConfig,
    tokenizer::JapaneseTokenizer,
};
use log;
use notify::{Event, RecursiveMode, Watcher};
use std::{
    path::Path,
    sync::{mpsc::channel, Arc, Mutex},
};

/// Watch a directory for changes and incrementally re-index.
pub struct VaultWatcher {
    db: Arc<Mutex<NoteDatabase>>,
    tokenizer: Arc<JapaneseTokenizer>,
    config: IndexConfig,
    embedder: Option<Embedder>,
}

impl VaultWatcher {
    pub fn new(
        db: Arc<Mutex<NoteDatabase>>,
        tokenizer: Arc<JapaneseTokenizer>,
        config: IndexConfig,
        embedder: Option<Embedder>,
    ) -> Self {
        Self {
            db,
            tokenizer,
            config,
            embedder,
        }
    }

    /// ファイル監視ループを開始する（Ctrl+C まで継続）。
    /// ウォッチャーはここで一度だけ生成する。
    pub fn watch(&self) -> Result<(), Box<dyn std::error::Error>> {
        let (tx, rx) = channel();

        let mut watcher = notify::recommended_watcher(move |res: Result<Event, notify::Error>| {
            if let Ok(event) = res {
                let _ = tx.send(event);
            }
        })?;

        watcher.watch(&self.config.notes_dir, RecursiveMode::Recursive)?;
        eprintln!(
            "Watching {} for changes...",
            self.config.notes_dir.display()
        );

        loop {
            match rx.recv() {
                Ok(event) => self.handle_event(&event)?,
                Err(e) => {
                    eprintln!("Watch error: {}", e);
                    break;
                }
            }
        }

        Ok(())
    }

    /// Returns `true` if `path` resolves within `notes_dir` after symlink resolution.
    /// Uses the same canonicalize + starts_with pattern as `search.rs` and `handler.rs`.
    fn is_path_within_vault(&self, path: &Path) -> bool {
        let vault_canonical = match self.config.notes_dir.canonicalize() {
            Ok(c) => c,
            Err(_) => return false,
        };
        let file_canonical = match path.canonicalize() {
            Ok(c) => c,
            Err(_) => return false,
        };
        file_canonical.starts_with(&vault_canonical)
    }

    fn handle_event(&self, event: &Event) -> Result<(), Box<dyn std::error::Error>> {
        use notify::event::{EventKind, ModifyKind, RenameMode};

        match event.kind {
            EventKind::Modify(ModifyKind::Data(_)) | EventKind::Create(_) => {
                for path in &event.paths {
                    // Symlink-safe vault check: resolve path to detect symlink escapes
                    if !self.is_path_within_vault(path) {
                        log::warn!(
                            "watcher: path outside vault (symlink?), skipping: {}",
                            path.display()
                        );
                        continue;
                    }
                    if let Ok(rel) = path.strip_prefix(&self.config.notes_dir) {
                        let rel_str = rel.to_string_lossy();
                        let db = self.db.lock().unwrap();
                        if let IndexResult::Error(e) =
                            index_file_with_embedder(&db, &self.tokenizer, self.embedder.as_ref(), path, &rel_str, &self.config)
                        {
                            log::warn!("watcher: failed to index {}: {}", rel_str, e);
                        }
                    }
                }
            }
            EventKind::Remove(_) => {
                for path in &event.paths {
                    if let Ok(rel) = path.strip_prefix(&self.config.notes_dir) {
                        let rel_str = rel.to_string_lossy();
                        let db = self.db.lock().unwrap();
                        if let Err(e) = db.delete_chunks_for_file(&rel_str) {
                            log::warn!("watcher: failed to delete chunks for {}: {}", rel_str, e);
                        }
                        if let Err(e) = db.delete_file_cache(&rel_str) {
                            log::warn!("watcher: failed to delete cache for {}: {}", rel_str, e);
                        }
                    }
                }
            }
            EventKind::Modify(ModifyKind::Name(RenameMode::Both)) if event.paths.len() == 2 => {
                {
                    let old = &event.paths[0];
                    let new = &event.paths[1];
                    // Only delete old path if it resolved within the vault
                    if self.is_path_within_vault(old) {
                        if let Ok(old_rel) = old.strip_prefix(&self.config.notes_dir) {
                            let rel_str = old_rel.to_string_lossy();
                            let db = self.db.lock().unwrap();
                            if let Err(e) = db.delete_chunks_for_file(&rel_str) {
                                log::warn!("watcher: failed to delete old path {}: {}", rel_str, e);
                            }
                            let _ = db.delete_file_cache(&rel_str);
                        }
                    }
                    // Symlink-safe vault check for the new path before indexing
                    if self.is_path_within_vault(new) {
                        if let Ok(new_rel) = new.strip_prefix(&self.config.notes_dir) {
                            let db = self.db.lock().unwrap();
                            if let IndexResult::Error(e) = index_file_with_embedder(
                                &db,
                                &self.tokenizer,
                                self.embedder.as_ref(),
                                new,
                                &new_rel.to_string_lossy(),
                                &self.config,
                            ) {
                                log::warn!(
                                    "watcher: failed to index new path {}: {}",
                                    new_rel.to_string_lossy(),
                                    e
                                );
                            }
                        }
                    } else {
                        log::warn!(
                            "watcher: renamed path outside vault, skipping: {}",
                            new.display()
                        );
                    }
                }
            }
            _ => {}
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{db::NoteDatabase, tokenizer::TokenizerConfig};
    use notify::event::{EventKind, ModifyKind};
    use notify::Event as NotifyEvent;
    use tempfile::TempDir;

    #[test]
    fn test_watcher_setup() {
        let tokenizer = crate::require_tokenizer!(TokenizerConfig::default());
        let temp = TempDir::new().unwrap();
        let db = Arc::new(Mutex::new(NoteDatabase::open_in_memory().unwrap()));
        let tokenizer = Arc::new(tokenizer);
        let config = IndexConfig {
            notes_dir: temp.path().to_path_buf(),
            ..Default::default()
        };
        let _watcher = VaultWatcher::new(db, tokenizer, config, None);
    }

    #[test]
    fn test_handle_event_modify_outside_vault_safe_noop() {
        let tokenizer = match JapaneseTokenizer::new(TokenizerConfig::default()) {
            Ok(tok) => Arc::new(tok),
            Err(_) => return,
        };
        let temp = TempDir::new().unwrap();
        let vault = temp.path().join("vault");
        std::fs::create_dir_all(&vault).unwrap();
        let db = Arc::new(Mutex::new(NoteDatabase::open_in_memory().unwrap()));
        let config = IndexConfig {
            notes_dir: vault,
            ..Default::default()
        };
        let watcher = VaultWatcher::new(Arc::clone(&db), Arc::clone(&tokenizer), config, None);
        let outside_dir = TempDir::new().unwrap();
        let outside_file = outside_dir.path().join("outside.md");
        std::fs::write(&outside_file, "content").unwrap();
        let event = NotifyEvent {
            kind: EventKind::Modify(ModifyKind::Data(notify::event::DataChange::Content)),
            paths: vec![outside_file],
            attrs: notify::event::EventAttributes::default(),
        };
        assert!(watcher.handle_event(&event).is_ok());
    }

    #[test]
    fn test_is_path_within_vault_rejects_symlink_escape() {
        let tokenizer = match JapaneseTokenizer::new(TokenizerConfig::default()) {
            Ok(tok) => Arc::new(tok),
            Err(_) => return,
        };
        let temp = TempDir::new().unwrap();
        let vault = temp.path().join("vault");
        std::fs::create_dir_all(&vault).unwrap();
        let db = Arc::new(Mutex::new(NoteDatabase::open_in_memory().unwrap()));
        let config = IndexConfig {
            notes_dir: vault.clone(),
            ..Default::default()
        };
        let watcher = VaultWatcher::new(db, Arc::clone(&tokenizer), config, None);
        let outside = vault.parent().unwrap().join("secret.txt");
        std::fs::write(&outside, "outside").unwrap();
        let link = vault.join("evil_link.md");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&outside, &link).unwrap();
        #[cfg(unix)]
        assert!(!watcher.is_path_within_vault(&link));
        #[cfg(not(unix))]
        let _ = link;
    }

    #[test]
    fn test_is_path_within_vault_accepts_symlink_inside_vault() {
        let tokenizer = match JapaneseTokenizer::new(TokenizerConfig::default()) {
            Ok(tok) => Arc::new(tok),
            Err(_) => return,
        };
        let temp = TempDir::new().unwrap();
        let vault = temp.path().join("vault");
        std::fs::create_dir_all(&vault).unwrap();
        let db = Arc::new(Mutex::new(NoteDatabase::open_in_memory().unwrap()));
        let config = IndexConfig {
            notes_dir: vault.clone(),
            ..Default::default()
        };
        let watcher = VaultWatcher::new(db, tokenizer, config, None);
        let real_file = vault.join("real.md");
        std::fs::write(&real_file, "content").unwrap();
        let link = vault.join("alias.md");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&real_file, &link).unwrap();
        #[cfg(unix)]
        assert!(watcher.is_path_within_vault(&link));
        #[cfg(not(unix))]
        let _ = link;
    }

    #[test]
    fn test_is_path_within_vault_nonexistent_path_returns_false() {
        let tokenizer = match JapaneseTokenizer::new(TokenizerConfig::default()) {
            Ok(tok) => Arc::new(tok),
            Err(_) => return,
        };
        let temp = TempDir::new().unwrap();
        let vault = temp.path().join("vault");
        std::fs::create_dir_all(&vault).unwrap();
        let db = Arc::new(Mutex::new(NoteDatabase::open_in_memory().unwrap()));
        let config = IndexConfig {
            notes_dir: vault.clone(),
            ..Default::default()
        };
        let watcher = VaultWatcher::new(db, tokenizer, config, None);
        let nonexistent = vault.join("nonexistent.md");
        assert!(!watcher.is_path_within_vault(&nonexistent));
    }

    #[test]
    fn test_handle_event_rename_reindexes_new_path() {
        use notify::event::RenameMode;

        let tokenizer = match JapaneseTokenizer::new(TokenizerConfig::default()) {
            Ok(tok) => Arc::new(tok),
            Err(_) => return,
        };
        let temp = TempDir::new().unwrap();
        let vault = temp.path().join("vault");
        std::fs::create_dir_all(&vault).unwrap();

        // Create source file and index it directly first
        let src_path = vault.join("old_name.md");
        std::fs::write(&src_path, "# Old name\n\nContent here.").unwrap();

        let db = Arc::new(Mutex::new(NoteDatabase::open_in_memory().unwrap()));
        let config = IndexConfig {
            notes_dir: vault.clone(),
            ..Default::default()
        };

        // Pre-index the old name file
        {
            let db = db.lock().unwrap();
            let _ = index_file_with_embedder(
                &db, &tokenizer, None, &src_path, "old_name.md", &config,
            );
        }
        assert_eq!(db.lock().unwrap().stats().unwrap().total_files, 1);

        // Create rename event: old_name.md -> new_name.md
        let new_path = vault.join("new_name.md");
        let event = NotifyEvent {
            kind: EventKind::Modify(ModifyKind::Name(RenameMode::Both)),
            paths: vec![src_path.clone(), new_path.clone()],
            attrs: notify::event::EventAttributes::default(),
        };

        let watcher = VaultWatcher::new(
            Arc::clone(&db),
            Arc::clone(&tokenizer),
            config,
            None,
        );

        // Rename the file on disk (the watcher code reads from disk)
        std::fs::rename(&src_path, &new_path).unwrap();

        // Handle the rename event
        watcher.handle_event(&event).unwrap();

        // Verify: old path should no longer be in DB
        let db = db.lock().unwrap();
        assert_eq!(db.cached_hash("old_name.md").unwrap(), None,
            "old path should be deleted from cache");

        // Verify: new path should be indexed
        assert!(db.cached_hash("new_name.md").unwrap().is_some(),
            "new path should be indexed");
        let stats = db.stats().unwrap();
        assert_eq!(stats.total_files, 1,
            "should have exactly 1 file indexed");
    }

    #[test]
    fn test_handle_event_create_indexes_new_file() {
        let tokenizer = match JapaneseTokenizer::new(TokenizerConfig::default()) {
            Ok(tok) => Arc::new(tok),
            Err(_) => return,
        };
        let temp = TempDir::new().unwrap();
        let vault = temp.path().join("vault");
        std::fs::create_dir_all(&vault).unwrap();
        let db = Arc::new(Mutex::new(NoteDatabase::open_in_memory().unwrap()));
        let config = IndexConfig {
            notes_dir: vault.clone(),
            ..Default::default()
        };
        let watcher = VaultWatcher::new(
            Arc::clone(&db),
            Arc::clone(&tokenizer),
            config,
            None,
        );

        // Create a file inside the vault
        let new_file = vault.join("new_file.md");
        std::fs::write(&new_file, "# New file\n\nContent for create event.").unwrap();
        let event = NotifyEvent {
            kind: EventKind::Create(notify::event::CreateKind::File),
            paths: vec![new_file],
            attrs: notify::event::EventAttributes::default(),
        };

        watcher.handle_event(&event).unwrap();

        // Verify the file was indexed
        let db = db.lock().unwrap();
        let stats = db.stats().unwrap();
        assert_eq!(stats.total_files, 1, "Create event should index the file");
    }

    #[test]
    fn test_handle_event_remove_deletes_from_db() {
        let tokenizer = match JapaneseTokenizer::new(TokenizerConfig::default()) {
            Ok(tok) => Arc::new(tok),
            Err(_) => return,
        };
        let temp = TempDir::new().unwrap();
        let vault = temp.path().join("vault");
        std::fs::create_dir_all(&vault).unwrap();

        let src_path = vault.join("to_delete.md");
        std::fs::write(&src_path, "# To delete\n\nContent.").unwrap();

        let db = Arc::new(Mutex::new(NoteDatabase::open_in_memory().unwrap()));
        let config = IndexConfig {
            notes_dir: vault.clone(),
            ..Default::default()
        };

        // Pre-index the file
        {
            let db = db.lock().unwrap();
            let _ = index_file_with_embedder(
                &db, &tokenizer, None, &src_path, "to_delete.md", &config,
            );
        }
        assert_eq!(db.lock().unwrap().stats().unwrap().total_files, 1);

        // Create Remove event
        let event = NotifyEvent {
            kind: EventKind::Remove(notify::event::RemoveKind::File),
            paths: vec![src_path],
            attrs: notify::event::EventAttributes::default(),
        };

        let watcher = VaultWatcher::new(
            Arc::clone(&db),
            Arc::clone(&tokenizer),
            config,
            None,
        );

        watcher.handle_event(&event).unwrap();

        // Verify the file was removed from DB
        let db = db.lock().unwrap();
        assert_eq!(db.cached_hash("to_delete.md").unwrap(), None,
            "Remove event should delete file from cache");
        let stats = db.stats().unwrap();
        assert_eq!(stats.total_files, 0, "Remove event should leave 0 files");
    }

    #[test]
    fn test_handle_event_modify_any_data_triggers_reindex() {
        let tokenizer = match JapaneseTokenizer::new(TokenizerConfig::default()) {
            Ok(tok) => Arc::new(tok),
            Err(_) => return,
        };
        let temp = TempDir::new().unwrap();
        let vault = temp.path().join("vault");
        std::fs::create_dir_all(&vault).unwrap();

        let src_path = vault.join("update.md");
        std::fs::write(&src_path, "# Update\n\nContent v1.").unwrap();

        let db = Arc::new(Mutex::new(NoteDatabase::open_in_memory().unwrap()));
        let config = IndexConfig {
            notes_dir: vault.clone(),
            ..Default::default()
        };

        // Pre-index the file
        {
            let db = db.lock().unwrap();
            let _ = index_file_with_embedder(
                &db, &tokenizer, None, &src_path, "update.md", &config,
            );
        }
        assert_eq!(db.lock().unwrap().stats().unwrap().total_files, 1);

        // Modify content on disk
        std::fs::write(&src_path, "# Update\n\nContent v2 (modified).").unwrap();

        // Fire DataChange::Any event
        let event = NotifyEvent {
            kind: EventKind::Modify(ModifyKind::Data(notify::event::DataChange::Any)),
            paths: vec![src_path],
            attrs: notify::event::EventAttributes::default(),
        };

        let watcher = VaultWatcher::new(
            Arc::clone(&db),
            Arc::clone(&tokenizer),
            config,
            None,
        );

        watcher.handle_event(&event).unwrap();

        // File should still be indexed (re-indexed)
        assert!(db.lock().unwrap().cached_hash("update.md").unwrap().is_some(),
            "file should still be in cache after modify event");
    }
}
