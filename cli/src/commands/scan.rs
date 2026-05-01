use clap::Args;
use obsidian_shiotsuchi_vault_core::{
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

pub fn run_scan(
    _args: &ScanArgs,
    notes_dir: &Path,
    db_path: &Path,
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

/// テスト用: ポーリングウォッチャーで timeout 後に自動終了する。
/// macOS FSEvents は /private/tmp 以下でイベントを配信しない場合があるため
/// PollWatcher を使用する。
#[cfg(test)]
pub fn run_scan_for_test(
    notes_dir: &Path,
    db: &Arc<Mutex<NoteDatabase>>,
    timeout: std::time::Duration,
    ready: Arc<std::sync::atomic::AtomicBool>,
) -> Result<usize, Box<dyn std::error::Error + Send + Sync>> {
    use notify::{Event, PollWatcher, RecursiveMode, Watcher};
    use notify::Config as NotifyConfig;
    use obsidian_shiotsuchi_vault_core::indexer::index_file;
    use std::sync::mpsc::channel;

    let tokenizer = get_tokenizer()?;
    let config = IndexConfig { notes_dir: notes_dir.to_path_buf(), ..Default::default() };
    let (tx, rx) = channel();
    let poll_config = NotifyConfig::default()
        .with_poll_interval(std::time::Duration::from_millis(100));
    let mut watcher = PollWatcher::new(
        move |res: Result<Event, _>| {
            if let Ok(e) = res { let _ = tx.send(e); }
        },
        poll_config,
    )?;
    watcher.watch(notes_dir, RecursiveMode::Recursive)?;
    ready.store(true, std::sync::atomic::Ordering::SeqCst);

    let mut indexed = 0usize;
    let deadline = std::time::Instant::now() + timeout;
    while std::time::Instant::now() < deadline {
        if let Ok(event) = rx.recv_timeout(std::time::Duration::from_millis(50)) {
            use notify::event::{EventKind, ModifyKind};
            if matches!(event.kind, EventKind::Create(_) | EventKind::Modify(ModifyKind::Data(_))) {
                for path in &event.paths {
                    if let Ok(rel) = path.strip_prefix(notes_dir) {
                        let db = db.lock().unwrap();
                        let _ = index_file(&db, &tokenizer, path, &rel.to_string_lossy(), &config);
                        indexed += 1;
                    }
                }
            }
        }
    }
    Ok(indexed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, time::Duration};
    use tempfile::TempDir;

    #[test]
    fn test_scan_indexes_new_file() {
        use std::sync::atomic::{AtomicBool, Ordering};

        let temp = TempDir::new().unwrap();
        let db_file = temp.path().join("test.db");
        let db = Arc::new(Mutex::new(NoteDatabase::open(&db_file).unwrap()));
        let ready = Arc::new(AtomicBool::new(false));

        let db_clone = Arc::clone(&db);
        let vault = temp.path().to_path_buf();
        let ready_clone = Arc::clone(&ready);
        let handle = std::thread::spawn(move || {
            run_scan_for_test(&vault, &db_clone, Duration::from_millis(2000), ready_clone)
        });

        // ウォッチャーが起動するまで待つ
        while !ready.load(Ordering::SeqCst) {
            std::thread::sleep(Duration::from_millis(10));
        }
        std::thread::sleep(Duration::from_millis(100)); // macOS kqueue 安定待ち
        fs::write(temp.path().join("new.md"), "# New Note\n\nNew content.").unwrap();
        std::thread::sleep(Duration::from_millis(2500));

        let _ = handle.join();
        let count = db.lock().unwrap().stats().unwrap().total_notes;
        assert_eq!(count, 1, "expected 1 indexed note after file creation");
    }
}