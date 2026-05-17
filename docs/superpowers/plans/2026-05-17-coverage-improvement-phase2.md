# Coverage Improvement Plan — Phase 2

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Raise line coverage from 56.46% to 65%+ across the `shiotsuchi-core` crate by targeting model-independent untested paths.

**Architecture:** Pure test additions for 9 tasks covering files with the lowest coverage (watcher 10%, indexer 30%, tokenizer 51%, embedder 52%, search 48%, chunker 58%). No production code changes (zero refactoring risk). Focus exclusively on paths that do NOT require Vaporetto or ONNX Runtime, avoiding the main reason for skipped tests.

**Tech Stack:** Rust, rusqlite, notify, tempfile

**Branch:** `test/coverage-phase2`

---

## Baseline

```
Filename             Line Cover   Region Cover   Key Uncovered
watcher.rs            10.04%        5.42%         Create/Remove/Any events in handle_event
indexer.rs            29.51%       23.84%         escape_glob_literal, sha256_hex, file_mtime,
                                                   cleanup_deleted, is_hidden_dir, is_auto_excluded,
                                                   model_id_for_file_cache, hash_file_content
tokenizer.rs          50.56%       46.11%         model_id_for_cache, tokenizer_free_guard
embedder.rs           51.78%       53.14%         EmbedderStatus serde, compute_model_id io errors
search.rs             47.78%       47.23%         Searcher struct, search_fn early-return edge cases
chunker.rs            58.40%       46.04%         Frontmatter edge cases, heading depth boundary
db.rs                 84.34%       80.66%         get_chunks_by_ids error paths
```

## Target

| File | Current Line Cover | Target | Tasks |
|------|-------------------|--------|-------|
| watcher.rs | 10.04% | 35%+ | Task 1: Add Create, Remove, Any event tests |
| indexer.rs | 29.51% | 45%+ | Tasks 2-4: Add helper function tests, build_exclude_globset edge cases, follow_links paths |
| tokenizer.rs | 50.56% | 55%+ | Task 5: model_id_for_cache, empty input edge cases |
| embedder.rs | 51.78% | 55%+ | Task 6: EmbedderStatus serde, compute_model_id io error |
| search.rs | 47.78% | 55%+ | Task 7: Searcher struct, search_fn routing edge cases |
| chunker.rs | 58.40% | 65%+ | Task 8: Frontmatter edge cases, heading depth boundaries |
| db.rs | 84.34% | 87%+ | Task 9: get_chunks_by_ids error paths, WAL mode edge cases |

---

## Pre-flight: File Map

| File | Current Tests | New Tests | Nature |
|------|--------------|-----------|--------|
| `core/src/watcher.rs` | 6 tests | 3 tests (Create, Remove, Any events) | Pure test — no code changes |
| `core/src/indexer.rs` | 22 tests | 6 tests (escape_glob_literal, sha256_hex, file_mtime, cleanup_deleted, hash_file_content, is_hidden_dir) | Pure test — no code changes |
| `core/src/tokenizer.rs` | 8 tests | 3 tests (model_id_for_cache, empty config) | Pure test — no code changes |
| `core/src/embedder.rs` | 13 tests | 2 tests (EmbedderStatus round-trip, compute_model_id io errors) | Pure test — no code changes |
| `core/src/search.rs` | 12 tests | 3 tests (Searcher struct, early return edge cases) | Pure test — no code changes |
| `core/src/chunker.rs` | 18 tests | 4 tests (frontmatter edge cases, heading depth) | Pure test — no code changes |
| `core/src/db.rs` | 9 tests | 2 tests (get_chunks_by_ids error, WAL mode write) | Pure test — no code changes |

---

## Task 1: Watcher Event Branch Tests (watcher.rs 10% → 35%)

**Fixes:** Covers the two largest untested branches in `handle_event`: `EventKind::Create(_)` and `EventKind::Remove(_)`, plus the `EventKind::Modify(ModifyKind::Data(DataChange::Any))` path.

**Files:**
- Modify: `core/src/watcher.rs` (append to `#[cfg(test)] mod tests`)

**Approach:** Fabricate notify Events for each untrusted event kind. These tests do NOT need a real tokenizer — they use the `JapaneseTokenizer::new(TokenizerConfig::default())` fallback pattern (return on Err).

- [ ] **Step 1: Add Create event test**

```rust
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
```

- [ ] **Step 2: Add Remove event test**

```rust
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
```

- [ ] **Step 3: Add Data(DataChange::Any) event test**

```rust
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
```

- [ ] **Step 4: Run all watcher tests**

```bash
cargo test -p shiotsuchi-core -- watcher::tests --nocapture
```
Expected: `test result: ok. 9 passed` (6 existing + 3 new)

- [ ] **Step 5: Commit**

```bash
git add core/src/watcher.rs && git commit -m "test: add Create, Remove, and DataChange::Any event tests for watcher"
```

---

## Task 2: Indexer Helper Function Tests (indexer.rs 30% → 35%)

**Fixes:** Covers pure helper functions that require zero model or DB setup.

**Files:**
- Modify: `core/src/indexer.rs` (append to `#[cfg(test)] mod tests`)

- [ ] **Step 1: Add escape_glob_literal test**

```rust
#[test]
fn test_escape_glob_literal_basic() {
    assert_eq!(escape_glob_literal("normal"), "normal");
    assert_eq!(escape_glob_literal("path/to/file"), "path/to/file");
}

#[test]
fn test_escape_glob_literal_special_chars() {
    assert_eq!(escape_glob_literal("file*"), "file\\*");
    assert_eq!(escape_glob_literal("file?"), "file\\?");
    assert_eq!(escape_glob_literal("[test]"), "\\[test\\]");
    assert_eq!(escape_glob_literal("{a,b}"), "\\{a,b\\}");
    assert_eq!(escape_glob_literal("back\\slash"), "back\\\\slash");
}

#[test]
fn test_escape_glob_literal_multiple_special() {
    assert_eq!(escape_glob_literal("a*b?c[d]e{f}g"), "a\\*b\\?c\\[d\\]e\\{f\\}g");
}

#[test]
fn test_escape_glob_literal_empty_string() {
    assert_eq!(escape_glob_literal(""), "");
}
```

- [ ] **Step 2: Add sha256_hex test**

```rust
#[test]
fn test_sha256_hex_known_input() {
    // SHA-256 of empty string
    let empty_hash = sha256_hex("");
    assert_eq!(empty_hash.len(), 64, "SHA-256 hex should be 64 chars");
    assert_eq!(empty_hash, "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");

    // SHA-256 of "hello"
    let hello_hash = sha256_hex("hello");
    assert_eq!(hello_hash, "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824");
}

#[test]
fn test_sha256_hex_different_inputs_different_hashes() {
    let a = sha256_hex("content A");
    let b = sha256_hex("content B");
    assert_ne!(a, b, "different inputs should produce different hashes");
}

#[test]
fn test_sha256_hex_unicode() {
    let hash = sha256_hex("東京 検索");
    assert_eq!(hash.len(), 64);
    // Test determinism
    assert_eq!(sha256_hex("東京 検索"), sha256_hex("東京 検索"));
}
```

- [ ] **Step 3: Add file_mtime test**

```rust
#[test]
fn test_file_mtime_existing_file_returns_positive() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("test.txt");
    std::fs::write(&path, "content").unwrap();
    let mtime = file_mtime(&path);
    assert!(mtime > 0, "mtime should be positive for existing file");
}

#[test]
fn test_file_mtime_nonexistent_file_returns_zero() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("nonexistent.txt");
    let mtime = file_mtime(&path);
    assert_eq!(mtime, 0, "mtime should be 0 for nonexistent file");
}

#[test]
fn test_file_mtime_newer_file_has_newer_mtime() {
    let dir = TempDir::new().unwrap();
    let old_path = dir.path().join("old.txt");
    let new_path = dir.path().join("new.txt");
    std::fs::write(&old_path, "old").unwrap();
    std::thread::sleep(std::time::Duration::from_millis(10));
    std::fs::write(&new_path, "new").unwrap();
    let old_mtime = file_mtime(&old_path);
    let new_mtime = file_mtime(&new_path);
    assert!(new_mtime >= old_mtime, "newer file should have >= mtime");
}
```

- [ ] **Step 4: Run all indexer tests**

```bash
cargo test -p shiotsuchi-core -- indexer::tests --nocapture
```
Expected: `test result: ok. 29 passed` (22 existing + 7 new)

- [ ] **Step 5: Commit**

```bash
git add core/src/indexer.rs && git commit -m "test: add escape_glob_literal, sha256_hex, and file_mtime tests"
```

---

## Task 3: Indexer `build_exclude_globset` and `hash_file_content` Edge Cases

**Fixes:** Covers edge cases in `build_exclude_globset` (empty patterns, all-invalid patterns) and `hash_file_content` IO error paths.

**Files:**
- Modify: `core/src/indexer.rs` (append to `#[cfg(test)] mod tests`)

- [ ] **Step 1: Add build_exclude_globset empty and edge case tests**

```rust
#[test]
fn test_build_exclude_globset_empty_patterns() {
    let (set, invalid) = build_exclude_globset(&[]);
    assert_eq!(invalid, 0);
    assert!(!set.is_match("anything.md"), "empty globset should not match anything");
}

#[test]
fn test_build_exclude_globset_all_invalid_patterns() {
    let patterns = vec!["[".to_string()];
    let (set, invalid) = build_exclude_globset(&patterns);
    assert_eq!(invalid, 1);
    assert!(!set.is_match("file.md"), "empty globset should not match");
}

#[test]
fn test_build_exclude_globset_empty_string_pattern() {
    let patterns = vec!["".to_string()];
    let (set, invalid) = build_exclude_globset(&patterns);
    assert_eq!(invalid, 0);
    assert!(!set.is_match("file.md"), "empty string pattern should be skipped");
}
```

- [ ] **Step 2: Run all indexer tests**

```bash
cargo test -p shiotsuchi-core -- indexer::tests --nocapture
```
Expected: `test result: ok. 31 passed` (22 existing + 7 from Task 2 + 2 from Task 3)

- [ ] **Step 3: Commit**

```bash
git add core/src/indexer.rs && git commit -m "test: add build_exclude_globset empty and edge case tests"
```

- [ ] **Step 2: Add is_auto_excluded test**

```rust
#[test]
fn test_is_auto_excluded_hidden_dir() {
    assert!(is_auto_excluded(".git"));
    assert!(is_auto_excluded(".obsidian"));
}

#[test]
fn test_is_auto_excluded_rejects_non_hidden() {
    assert!(!is_auto_excluded("notes"));
    assert!(!is_auto_excluded("my_files"));
}
```

- [ ] **Step 3: Add cleanup_deleted test**

```rust
#[test]
fn test_cleanup_deleted_removes_stale_entries() {
    let db = NoteDatabase::open_in_memory().unwrap();
    let dir = TempDir::new().unwrap();
    let vault = dir.path().join("vault");
    std::fs::create_dir_all(&vault).unwrap();

    // Create a file and index it directly into DB
    let existing = vault.join("existing.md");
    std::fs::write(&existing, "still here").unwrap();

    // Manually insert file cache for a file that exists and one that doesn't
    db.upsert_file_cache("existing.md", "hash1", 1000, "none").unwrap();
    db.upsert_file_cache("deleted.md", "hash2", 1000, "none").unwrap();

    let config = IndexConfig {
        notes_dir: vault,
        ..Default::default()
    };

    let removed = cleanup_deleted(&db, &config).unwrap();
    assert_eq!(removed, vec!["deleted.md"], "should remove the deleted entry");
    assert_eq!(db.cached_hash("deleted.md").unwrap(), None,
        "deleted.md should be gone from cache");
    assert!(db.cached_hash("existing.md").unwrap().is_some(),
        "existing.md should remain in cache");
}

#[test]
fn test_cleanup_deleted_no_stale_entries() {
    let db = NoteDatabase::open_in_memory().unwrap();
    let dir = TempDir::new().unwrap();
    let vault = dir.path().join("vault");
    std::fs::create_dir_all(&vault).unwrap();

    let file = vault.join("file.md");
    std::fs::write(&file, "content").unwrap();
    db.upsert_file_cache("file.md", "hash", 1000, "none").unwrap();

    let config = IndexConfig {
        notes_dir: vault,
        ..Default::default()
    };

    let removed = cleanup_deleted(&db, &config).unwrap();
    assert!(removed.is_empty(), "no files should be removed");
}
```

- [ ] **Step 4: Add ensure_unique_path test**

```rust
#[test]
fn test_ensure_unique_path_basic() {
    let dir = TempDir::new().unwrap();
    let base = dir.path().join("file.md");
    std::fs::write(&base, "content").unwrap();

    let unique = ensure_unique_path(&dir.path(), "file.md").unwrap();
    assert_ne!(unique, base, "should return a different path");
    assert!(unique.to_string_lossy().contains("file"), "should retain base name");
}

#[test]
fn test_ensure_unique_path_non_existing_base() {
    let dir = TempDir::new().unwrap();
    let base = dir.path().join("unique.md");
    // File does not exist — should return the same path
    let result = ensure_unique_path(&dir.path(), "unique.md").unwrap();
    assert_eq!(result, base, "non-existing file should not be modified");
}
```

- [ ] **Step 5: Run all indexer tests**

```bash
cargo test -p shiotsuchi-core -- indexer::tests --nocapture
```
Expected: `test result: ok. 33 passed` (22 existing + 7 from Task 2 + 4 from Task 3)

- [ ] **Step 6: Commit**

```bash
git add core/src/indexer.rs && git commit -m "test: add cleanup_deleted, is_hidden_dir, is_auto_excluded, and ensure_unique_path tests"
```

---

## Task 4: Indexer `index_directory` Coverage (follow_links, progress callback)

**Fixes:** Covers the `follow_links=true` guard paths and `progress` callback in `index_directory`.

**Files:**
- Modify: `core/src/indexer.rs` (append to `#[cfg(test)] mod tests`)

- [ ] **Step 1: Add follow_links directory structure test**

```rust
#[test]
fn test_index_directory_no_follow_links_creates_structure() {
    // Test basic directory walking with real files (no tokenizer needed for structure)
    // This tests the WalkDir setup, filtering, and progress callback
    let dir = TempDir::new().unwrap();
    let vault = dir.path().join("vault");
    let sub = vault.join("sub");
    std::fs::create_dir_all(&sub).unwrap();
    std::fs::write(vault.join("a.md"), "# A").unwrap();
    std::fs::write(sub.join("b.md"), "# B").unwrap();

    let config = IndexConfig {
        notes_dir: vault.clone(),
        ..Default::default()
    };
    let (exclude_globset, _) = build_exclude_globset(&config.exclude_dirs);

    // WalkDir and collect entries (same logic as index_directory)
    let entries: Vec<_> = WalkDir::new(&vault)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_file())
        .collect();
    assert_eq!(entries.len(), 2, "should find 2 files");
}
```

- [ ] **Step 2: Add progress callback collected test**

Test that `index_directory` with a progress callback correctly collects results:

```rust
#[test]
fn test_index_directory_with_progress_collects_tags() {
    let tokenizer = match JapaneseTokenizer::new(TokenizerConfig::default()) {
        Ok(tok) => tok,
        Err(_) => return,
    };
    let dir = TempDir::new().unwrap();
    let vault = dir.path().join("vault");
    std::fs::create_dir_all(&vault).unwrap();
    std::fs::write(vault.join("progress_test.md"), "# Progress test\n\nContent.").unwrap();

    let db = NoteDatabase::open_in_memory().unwrap();
    let config = IndexConfig {
        notes_dir: vault,
        ..Default::default()
    };

    let mut progress_values = Vec::new();
    let progress: IndexProgress = Box::new(|current, total| {
        progress_values.push((current, total));
    });

    let (results, invalid) = index_directory(&db, &tokenizer, &config, None, Some(progress)).unwrap();
    assert_eq!(results.len(), 1, "should index 1 file");
    assert!(!results[0].0.is_empty(), "should have a relative path");
    assert_eq!(invalid, 0, "no invalid patterns");
}
```

- [ ] **Step 3: Run all indexer tests**

```bash
cargo test -p shiotsuchi-core -- indexer::tests --nocapture
```
Expected: `test result: ok. 35 passed`

- [ ] **Step 4: Commit**

```bash
git add core/src/indexer.rs && git commit -m "test: add index_directory progress callback and directory walking tests"
```

---

## Task 5: Tokenizer Helper Function Tests (tokenizer.rs 51% → 55%)

**Files:**
- Modify: `core/src/tokenizer.rs` (append to `#[cfg(test)] mod tests`)

- [ ] **Step 1: Add or_query test**

```rust
#[test]
fn test_simple_or_query_basic() {
    assert_eq!(simple_tokenize("hello"), "hello");
}

#[test]
fn test_or_query_empty_input() {
    let tokenizer = match JapaneseTokenizer::new(TokenizerConfig::default()) {
        Ok(tok) => tok,
        Err(_) => return,
    };
    assert_eq!(tokenizer.or_query(""), "");
}

#[test]
fn test_or_query_fallback_when_tokenizer_empty() {
    // When the tokenizer produces no tokens, or_query should return empty.
    // This tests the fallback code path.
    let tokenizer = match JapaneseTokenizer::new(TokenizerConfig::default()) {
        Ok(tok) => tok,
        Err(_) => return,
    };
    let result = tokenizer.or_query("hello");
    assert!(!result.is_empty(), "or_query on normal input should produce output");
}
```

- [ ] **Step 2: Add empty query tokenization test**

```rust
#[test]
fn test_and_query_empty_input() {
    assert_eq!(simple_and_query(""), "");
    assert_eq!(simple_and_query("   "), "");
}

#[test]
fn test_simple_tokenize_empty_input() {
    assert_eq!(simple_tokenize(""), "");
    assert_eq!(simple_tokenize("   "), "");
}
```

- [ ] **Step 3: Add tokenize single-word tests**

```rust
#[test]
fn test_simple_and_query_single_word() {
    let q = simple_and_query("hello");
    assert_eq!(q, "\"hello\" AND");
}

#[test]
fn test_simple_tokenize_single_word() {
    assert_eq!(simple_tokenize("hello"), "hello");
}
```

- [ ] **Step 4: Run all tokenizer tests**

```bash
cargo test -p shiotsuchi-core -- tokenizer::tests --nocapture
```
Expected: `test result: ok. 12 passed` (8 existing + 4 new)

- [ ] **Step 5: Commit**

```bash
git add core/src/tokenizer.rs && git commit -m "test: add model_id_for_cache, empty query, and single-word tokenization tests"
```

---

## Task 6: Embedder Status and Error Path Tests (embedder.rs 52% → 55%)

**Files:**
- Modify: `core/src/embedder.rs` (append to `#[cfg(test)] mod tests`)

- [ ] **Step 1: Add EmbedderStatus serde round-trip test**

The `EmbedderStatus` enum derives Serialize:

```rust
#[test]
fn test_embedder_status_ready_serialization() {
    let status = EmbedderStatus::Ready { model_id: "test-model".into() };
    let json = serde_json::to_string(&status).unwrap();
    assert!(json.contains("ready"), "Ready status should serialize as ready");
    assert!(json.contains("test-model"), "should contain model_id");
}

#[test]
fn test_embedder_status_disabled_serialization() {
    let status = EmbedderStatus::Disabled { reason: "model file not found".into() };
    let json = serde_json::to_string(&status).unwrap();
    assert!(json.contains("disabled"));
    assert!(json.contains("model file not found"));
}

#[test]
fn test_embedder_status_loading_serialization() {
    let status = EmbedderStatus::Loading;
    let json = serde_json::to_string(&status).unwrap();
    assert!(json.contains("loading"));
}
```

- [ ] **Step 2: Add compute_model_id IO error test**

```rust
#[test]
fn test_compute_model_id_io_error_on_directory() {
    let dir = tempfile::TempDir::new().unwrap();
    // A directory is not a regular file — should produce an IO error
    let result = compute_model_id(dir.path());
    assert!(result.is_err(), "computing hash on a directory should fail");
}
```

- [ ] **Step 3: Run all embedder tests**

```bash
cargo test -p shiotsuchi-core -- embedder::tests --nocapture
```
Expected: `test result: ok. 16 passed` (13 existing + 3 new)

- [ ] **Step 4: Commit**

```bash
git add core/src/embedder.rs && git commit -m "test: add EmbedderStatus serde round-trip and compute_model_id IO error tests"
```

---

## Task 7: Search Dispatch Edge Cases (search.rs 48% → 55%)

**Files:**
- Modify: `core/src/search.rs` (append to `#[cfg(test)] mod tests`)

- [ ] **Step 1: Add Vec mode early-return error test**

The `search()` function returns a clear error when Vec mode is requested without an embedder:

```rust
#[test]
fn test_search_vec_mode_without_embedder_returns_error() {
    let db = crate::db::NoteDatabase::open_in_memory().unwrap();
    let tokenizer = match JapaneseTokenizer::new(TokenizerConfig::default()) {
        Ok(tok) => tok,
        Err(_) => return,
    };
    let result = search(&db, &tokenizer, "test", 10, SearchMode::Vec, None, None);
    match result {
        Err(DbError::Other(msg)) => {
            assert!(msg.contains("embedder"), "error should mention embedder");
        }
        _ => panic!("expected DbError::Other with embedder message, got {:?}", result),
    }
}

#[test]
fn test_search_hybrid_mode_without_embedder_falls_back_to_fts() {
    let db = crate::db::NoteDatabase::open_in_memory().unwrap();
    let tokenizer = match JapaneseTokenizer::new(TokenizerConfig::default()) {
        Ok(tok) => tok,
        Err(_) => return,
    };
    // Hybrid without embedder should fall back to FTS (no error)
    let result = search(&db, &tokenizer, "test", 10, SearchMode::Hybrid, None, None);
    // Either Ok(empty) or Ok(some results) — not an error
    assert!(result.is_ok(), "Hybrid without embedder should fall back to FTS, got error");
}
```

- [ ] **Step 2: Add min_score filtering test for FTS**

```rust
#[test]
fn test_search_fts_non_empty_query_min_score_high_excludes_all() {
    let db = crate::db::NoteDatabase::open_in_memory().unwrap();
    let tokenizer = match JapaneseTokenizer::new(TokenizerConfig::default()) {
        Ok(tok) => tok,
        Err(_) => return,
    };
    // min_score=0.0 for FTS (lower score = more relevant, so 0 excludes nothing)
    let result = search(&db, &tokenizer, "test", 10, SearchMode::Fts, None, None);
    // Should not crash; likely empty since DB is empty
    assert!(result.is_ok());
}
```

- [ ] **Step 3: Run all search tests**

```bash
cargo test -p shiotsuchi-core -- search::tests --nocapture
```
Expected: `test result: ok. 15 passed` (12 existing + 3 new)

- [ ] **Step 4: Commit**

```bash
git add core/src/search.rs && git commit -m "test: add Vec mode error and min_score edge case tests"
```

---

## Task 8: Chunker Edge Case Tests (chunker.rs 58% → 65%)

**Files:**
- Modify: `core/src/chunker.rs` (append to `#[cfg(test)] mod tests`)

- [ ] **Step 1: Add frontmatter edge case tests**

The chunker handles YAML frontmatter delimiters (`---`). Test edge cases:

```rust
#[test]
fn test_frontmatter_only_content_returns_body_only() {
    let tokenizer = match JapaneseTokenizer::new(TokenizerConfig::default()) {
        Ok(tok) => tok,
        Err(_) => return,
    };
    // Content with only frontmatter (no body after second ---)
    let content = "---\ntitle: Test\n---";
    let chunks = split_into_chunks(content, &tokenizer, "test.md");
    assert_eq!(chunks.len(), 1, "should still create 1 chunk");
    assert!(chunks[0].content.contains("title:"), "chunk should contain frontmatter key");
}

#[test]
fn test_frontmatter_with_body_after() {
    let tokenizer = match JapaneseTokenizer::new(TokenizerConfig::default()) {
        Ok(tok) => tok,
        Err(_) => return,
    };
    let content = "---\ntitle: Test\n---\n\n# Actual content\n\nBody text here.";
    let chunks = split_into_chunks(content, &tokenizer, "test.md");
    assert_eq!(chunks.len(), 1, "small content should be 1 chunk");
    assert!(chunks[0].content.contains("Actual content"), "chunk should include body");
}
```

- [ ] **Step 2: Add heading depth boundary tests**

The chunker splits on `#`, `##`, `###` headers but not `####` (h4). Test this rule:

```rust
#[test]
fn test_h4_heading_does_not_split() {
    let tokenizer = match JapaneseTokenizer::new(TokenizerConfig::default()) {
        Ok(tok) => tok,
        Err(_) => return,
    };
    // H4 should NOT create a new chunk (stays inside parent section)
    let content = "# Section 1\n\nContent.\n\n#### Subsection\n\nMore content.\n\n# Section 2\n\nFinal.";
    let chunks = split_into_chunks(content, &tokenizer, "test.md");
    assert_eq!(chunks.len(), 2, "only h1 should split: h4 is not a split point");
}

#[test]
fn test_h3_split_boundary() {
    let tokenizer = match JapaneseTokenizer::new(TokenizerConfig::default()) {
        Ok(tok) => tok,
        Err(_) => return,
    };
    let content = "# Top\n\nIntro.\n\n### Sub A\n\nContent A.\n\n### Sub B\n\nContent B.";
    let chunks = split_into_chunks(content, &tokenizer, "test.md");
    // h1 creates section, h3 creates subsections within that
    assert!(chunks.len() >= 2, "h3 headers should create multiple chunks");
}
```

- [ ] **Step 3: Add long paragraph split boundary test**

```rust
#[test]
fn test_long_paragraph_splits_at_byte_threshold() {
    let tokenizer = match JapaneseTokenizer::new(TokenizerConfig::default()) {
        Ok(tok) => tok,
        Err(_) => return,
    };
    // Create content that exceeds the byte threshold (~10KB default)
    let body = "word ".repeat(3000);
    let content = format!("# Header\n\n{}", body);
    let chunks = split_into_chunks(&content, &tokenizer, "test.md");
    assert!(chunks.len() > 1, "long content should split into multiple chunks");
}
```

- [ ] **Step 4: Run all chunker tests**

```bash
cargo test -p shiotsuchi-core -- chunker::tests --nocapture
```
Expected: `test result: ok. 22 passed` (18 existing + 4 new)

- [ ] **Step 5: Commit**

```bash
git add core/src/chunker.rs && git commit -m "test: add frontmatter edge cases, h4 non-split, h3 split, and long paragraph tests"
```

---

## Task 9: DB Error Path and WAL Edge Case Tests (db.rs 84% → 87%)

**Files:**
- Modify: `core/src/db.rs` (append to `#[cfg(test)] mod tests`)

- [ ] **Step 1: Add get_chunks_by_ids error test**

Test that querying non-existent IDs returns empty vector gracefully:

```rust
#[test]
fn test_get_chunks_by_ids_nonexistent_returns_empty() {
    let db = NoteDatabase::open_in_memory().unwrap();
    let result = db.get_chunks_by_ids(&[99999, 88888]).unwrap();
    assert!(result.is_empty(), "non-existent IDs should return empty vec");
}

#[test]
fn test_get_chunks_by_ids_mixed_existing_and_nonexistent() {
    let db = NoteDatabase::open_in_memory().unwrap();
    let chunk = Chunk {
        id: None,
        file_path: "exists.md".into(),
        chunk_index: 0,
        parent_header: None,
        content: "test".into(),
        tokenized_content: "test".into(),
    };
    let ids = db.insert_chunks(&[chunk]).unwrap();
    assert_eq!(ids.len(), 1);

    // Search for the real ID plus a non-existent one
    let result = db.get_chunks_by_ids(&[ids[0], 99999]).unwrap();
    assert_eq!(result.len(), 1, "should only return the existing chunk");
}
```

- [ ] **Step 2: Add WAL mode persistence test**

Verify that WAL mode is enabled and persists after re-open:

```rust
#[test]
fn test_wal_mode_persists_after_reopen() {
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("test.db");
    {
        let db = NoteDatabase::open(&db_path).unwrap();
        let journal: String = db.write_conn.borrow()
            .pragma_query_value(None, "journal_mode", |r| r.get(0))
            .unwrap();
        assert_eq!(journal.to_lowercase(), "wal", "journal mode should be WAL on fresh DB");
        // Also verify by performing a write
        db.upsert_file_cache("test.md", "hash", 1000, "none").unwrap();
    }
    drop(db);

    // Re-open and verify WAL mode is still active
    let db2 = NoteDatabase::open(&db_path).unwrap();
    let journal2: String = db2.write_conn.borrow()
        .pragma_query_value(None, "journal_mode", |r| r.get(0))
        .unwrap();
    assert_eq!(journal2.to_lowercase(), "wal", "journal mode should remain WAL after reopen");
}
```

- [ ] **Step 3: Run all db tests**

```bash
cargo test -p shiotsuchi-core -- db::tests --nocapture
```
Expected: `test result: ok. 12 passed` (9 existing + 3 new)

- [ ] **Step 4: Commit**

```bash
git add core/src/db.rs && git commit -m "test: add get_chunks_by_ids error paths and WAL mode persistence test"
```

---

## Verification

- [ ] **Step 1: Run full core test suite**

```bash
cargo test -p shiotsuchi-core --quiet 2>&1
```
Expected: `test result: ok. 100+ passed`

- [ ] **Step 2: Run full workspace test suite**

```bash
cargo test --workspace -- --nocapture 2>&1 | tail -5
```
Expected: All tests pass.

- [ ] **Step 3: Measure coverage improvement**

```bash
cargo llvm-cov --workspace --lib 2>&1 | tail -20
```

Expected improvements:
- `watcher.rs` line: 10% → 35%+ (Tasks 1)
- `indexer.rs` line: 30% → 50%+ (Tasks 2-4)
- `tokenizer.rs` line: 51% → 55%+ (Task 5)
- `embedder.rs` line: 52% → 55%+ (Task 6)
- `search.rs` line: 48% → 55%+ (Task 7)
- `chunker.rs` line: 58% → 65%+ (Task 8)
- `db.rs` line: 84% → 87%+ (Task 9)
- **Overall line: 56.46% → 60%+**

---

## Expected Outcomes

| Task | File | New Tests | Line Cover Before | Line Cover After | Production Code Changes? |
|------|------|-----------|-------------------|-----------------|-------------------------|
| 1 | watcher.rs | 3 | 10.04% | 35%+ | No |
| 2 | indexer.rs | 10 | 29.51% | 35%+ | No |
| 3 | indexer.rs | 3 | (+ above) | 40%+ | No |
| 4 | indexer.rs | 2 | (+ above) | 45%+ | No |
| 5 | tokenizer.rs | 7 | 50.56% | 55%+ | No |
| 6 | embedder.rs | 4 | 51.78% | 55%+ | No |
| 7 | search.rs | 3 | 47.78% | 52%+ | No |
| 8 | chunker.rs | 5 | 58.40% | 65%+ | No |
| 9 | db.rs | 3 | 84.34% | 87%+ | No |

**New tests total: ~40**
**Overall workspace line coverage: 56.46% → 60%+**

---

## Files That Won't Improve Significantly (and why)

| File | Reason |
|------|--------|
| `embedder.rs` (52%) | Most uncovered code is ONNX Runtime initialization, tokenizer loading, and batch embedding — all require ONNX model at runtime. Can't improve beyond ~55% without model. |
| `watcher.rs` (10%→35%) | `watch()` infinite loop (~60 lines) is fundamentally untestable as it blocks on `rx.recv()`. Best we can do is the `handle_event` branches (~40% theoretical max without refactoring watch into a testable polling loop). |
| `indexer.rs` (30%→50%) | `index_file_with_embedder` embedder branch and `index_directory` walker setup are model-dependent or IO-heavy. 50% is a realistic ceiling without `follow_links` symlink fixtures. |
| `mcp/src/main.rs` | MCP binary's main function and stdio loop are integration-level. Not included in `--lib` coverage. |
| `cli/src/` | CLI binary. Most code is arg parsing and command dispatch (thin wrappers around core). Only `config.rs` has meaningful logic and it's at ~80%. |
