# `shiotsuchi clean` Command Implementation Plan

> **Status:** Implemented (see `cli/src/commands/clean.rs`)
> **Date:** 2026-05-18
> **Completed:** 2026-05-18/19

> **For agentic workers:** Single-task implementation.

**Goal:** Add `shiotsuchi clean` — backup DB, delete, and re-index from scratch.

**Architecture:** New subcommand file `cli/src/commands/clean.rs` following the same pattern as `chart.rs`. Uses existing `NoteDatabase::open`, `index_directory`, and `run_chart`-style reporting.

---

### Actual Implementation Notes

The production implementation at `cli/src/commands/clean.rs` improves on the plan in several ways:

1. **Atomic rename** — indexes into a temp DB first, then renames over the original (vs. plan's backup→delete→re-index which has a longer window of data loss)
2. **Old backup pruning** — keeps only the 3 most recent backups
3. **Symlink protection** — refuses to delete symlinks during cleanup
4. **WAL checkpoint** — runs `wal_checkpoint()` before rename so all data is in the main file
5. **Comprehensive tests** — 8 test cases covering backup, delete, and full flow

---

### Task 1: Implement `shiotsuchi clean` subcommand

**Files:**
- Create: `cli/src/commands/clean.rs`
- Modify: `cli/src/commands/mod.rs`
- Modify: `cli/src/main.rs`

- [x] **Step 1: Create cli/src/commands/clean.rs**

```rust
use crate::config::IndexingConfig;
use clap::Args;
use shiotsuchi_core::indexer::index_directory;
use shiotsuchi_core::models::IndexConfig;
use shiotsuchi_core::tokenizer::get_tokenizer;
use shiotsuchi_core::db::NoteDatabase;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Args, Debug)]
pub struct CleanArgs {}

/// Backup a single file by copying it to `<path>.bak.<timestamp>`.
/// Returns the backup path if the original existed, None otherwise.
fn backup_file(path: &Path) -> Option<PathBuf> {
    if !path.exists() {
        return None;
    }
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let backup_name = format!("{}.bak.{}", path.to_string_lossy(), ts);
    let backup_path = PathBuf::from(&backup_name);
    match std::fs::copy(path, &backup_path) {
        Ok(_) => {
            // Preserve permissions on Unix
            #[cfg(unix)]
            if let Ok(meta) = std::fs::metadata(path) {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(&backup_path, meta.permissions());
            }
            Some(backup_path)
        }
        Err(e) => {
            eprintln!("Warning: failed to backup {}: {}", path.display(), e);
            None
        }
    }
}

fn delete_db_files(db_path: &Path) {
    // Main DB
    let _ = std::fs::remove_file(db_path);
    // WAL and SHM companions
    let base = db_path.to_string_lossy();
    for suffix in ["-wal", "-shm"] {
        let companion = PathBuf::from(format!("{}{}", base, suffix));
        let _ = std::fs::remove_file(&companion);
    }
}

pub fn run_clean(
    vaults: &[(String, PathBuf)],
    db_path: &Path,
    indexing_cfg: &IndexingConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    if !db_path.exists() {
        eprintln!("Error: database not found at {}", db_path.display());
        eprintln!("Run `shiotsuchi chart` to create the index first.");
        std::process::exit(1);
    }

    // Step 1: Backup
    let backed_up = backup_file(db_path);
    let base = db_path.to_string_lossy();
    for suffix in ["-wal", "-shm"] {
        let companion = PathBuf::from(format!("{}{}", base, suffix));
        backup_file(&companion);
    }

    // Step 2: Delete originals
    delete_db_files(db_path);

    // Step 3: Re-index from scratch
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    crate::util::secure_parent_dir(db_path);

    let db = NoteDatabase::open(db_path)?;
    let tokenizer = get_tokenizer()?;
    let config = IndexConfig {
        vaults: vaults.to_vec(),
        include_extensions: indexing_cfg.include_extensions.clone(),
        exclude_dirs: indexing_cfg.exclude_dirs.clone(),
        auto_exclude_hidden: indexing_cfg.auto_exclude_hidden,
        follow_links: indexing_cfg.follow_links,
        dynamic_threshold: indexing_cfg.dynamic_threshold,
    };

    let embedder = shiotsuchi_core::embedder::resolve_model_path(None)
        .and_then(|p| match shiotsuchi_core::embedder::Embedder::load(&p) {
            Ok(e) => {
                eprintln!("[info] Embedder model loaded — vector indexing enabled.");
                Some(e)
            }
            Err(e) => {
                eprintln!("[warn] Could not load embedder: {}.", e);
                None
            }
        });

    let (results, invalid_patterns) = index_directory(&db, &tokenizer, &config, embedder.as_ref(), None)?;

    let mut indexed = 0usize;
    let mut skipped = 0usize;
    let mut errors = 0usize;
    for (_, _, result) in &results {
        match result {
            shiotsuchi_core::indexer::IndexResult::Inserted | shiotsuchi_core::indexer::IndexResult::Updated => indexed += 1,
            shiotsuchi_core::indexer::IndexResult::Skipped => skipped += 1,
            shiotsuchi_core::indexer::IndexResult::Error(_) => errors += 1,
        }
    }

    if let Some(ref backup_path) = backed_up {
        println!("Backup saved to: {}", backup_path.display());
    }
    println!(
        "Re-indexed {} files ({} skipped, {} errors)",
        indexed, skipped, errors
    );
    if invalid_patterns > 0 {
        println!("  {} invalid pattern(s) in exclude_dirs", invalid_patterns);
    }

    Ok(())
}
```

- [x] **Step 2: Add module to mod.rs**

Add `pub mod clean;` to `cli/src/commands/mod.rs`:

```rust
pub mod chart;
pub mod clean;       // NEW
pub mod config;
pub mod config_migrate;
pub mod delete;
pub mod dive;
pub mod dredge;
pub mod init;
pub mod log;
pub mod noise;
pub mod scan;
pub mod setup;
pub mod support;
pub mod tide;
```

- [x] **Step 3: Add Commands variant in main.rs**

Add `Clean` to the Commands enum:
```rust
#[derive(Subcommand)]
enum Commands {
    Chart(commands::chart::ChartArgs),
    Clean(commands::clean::CleanArgs),    // NEW
    Config(commands::config::ConfigArgs),
    // ...
}
```

Add the dispatch arm in main():
```rust
Commands::Clean(_args) => {
    commands::clean::run_clean(&resolved_vaults, &db_path, &cfg.indexing)?;
}
```

- [x] **Step 4: Build and test**

```bash
cargo build 2>&1
cargo test -p shiotsuchi 2>&1
```

- [x] **Step 5: Commit**

```bash
git add cli/src/commands/clean.rs cli/src/commands/mod.rs cli/src/main.rs
git commit -m "feat(cli): add clean command (backup + re-index)"
```
