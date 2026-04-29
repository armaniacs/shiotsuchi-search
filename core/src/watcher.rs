use crate::{
    db::NoteDatabase,
    indexer::index_file,
    models::IndexConfig,
    tokenizer::JapaneseTokenizer,
};
use notify::{Event, RecursiveMode, Watcher};
use std::sync::{mpsc::channel, Arc, Mutex};

/// Watch a directory for changes and incrementally re-index.
pub struct VaultWatcher {
    db: Arc<Mutex<NoteDatabase>>,
    tokenizer: Arc<JapaneseTokenizer>,
    config: IndexConfig,
}

impl VaultWatcher {
    pub fn new(
        db: Arc<Mutex<NoteDatabase>>,
        tokenizer: Arc<JapaneseTokenizer>,
        config: IndexConfig,
    ) -> Self {
        Self { db, tokenizer, config }
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
        eprintln!("Watching {} for changes...", self.config.notes_dir.display());

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

    fn handle_event(&self, event: &Event) -> Result<(), Box<dyn std::error::Error>> {
        use notify::event::{EventKind, ModifyKind, RenameMode};

        match event.kind {
            EventKind::Modify(ModifyKind::Data(_)) | EventKind::Create(_) => {
                for path in &event.paths {
                    if let Ok(rel) = path.strip_prefix(&self.config.notes_dir) {
                        let rel_str = rel.to_string_lossy();
                        let db = self.db.lock().unwrap();
                        // tokenizer を渡して再インデックス
                        let _ = index_file(&db, &self.tokenizer, path, &rel_str, &self.config);
                    }
                }
            }
            EventKind::Remove(_) => {
                for path in &event.paths {
                    if let Ok(rel) = path.strip_prefix(&self.config.notes_dir) {
                        let db = self.db.lock().unwrap();
                        let _ = db.delete_note(&rel.to_string_lossy());
                    }
                }
            }
            EventKind::Modify(ModifyKind::Name(RenameMode::Both)) => {
                if event.paths.len() == 2 {
                    let old = &event.paths[0];
                    let new = &event.paths[1];
                    if let Ok(old_rel) = old.strip_prefix(&self.config.notes_dir) {
                        let db = self.db.lock().unwrap();
                        let _ = db.delete_note(&old_rel.to_string_lossy());
                    }
                    if let Ok(new_rel) = new.strip_prefix(&self.config.notes_dir) {
                        let db = self.db.lock().unwrap();
                        let _ = index_file(&db, &self.tokenizer, new, &new_rel.to_string_lossy(), &self.config);
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
    use crate::{db::NoteDatabase, tokenizer::{JapaneseTokenizer, TokenizerConfig}};
    use tempfile::TempDir;

    #[test]
    fn test_watcher_setup() {
        // Skip if tokenizer cannot be created
        let tokenizer = match JapaneseTokenizer::new(TokenizerConfig::default()) {
            Ok(tok) => tok,
            Err(_) => return, // Skip test if model not available
        };

        let temp = TempDir::new().unwrap();
        let db = Arc::new(Mutex::new(NoteDatabase::open_in_memory().unwrap()));
        let tokenizer = Arc::new(tokenizer);
        let config = IndexConfig {
            notes_dir: temp.path().to_path_buf(),
            ..Default::default()
        };

        // VaultWatcher は new() で失敗しない（ウォッチャー生成は watch() 内）
        let _watcher = VaultWatcher::new(db, tokenizer, config);
    }
}
