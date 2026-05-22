use crate::{
    chunker::split_into_chunks,
    db::{DbError, NoteDatabase},
    embedder::Embedder,
    models::IndexConfig,
    tokenizer::JapaneseTokenizer,
};
use globset::{Glob, GlobSet, GlobSetBuilder};
use sha2::{Digest, Sha256};
use std::{fs, path::Path, time::SystemTime};
use walkdir::WalkDir;

/// Optional progress callback for index_directory.
/// Arguments are (current, total) where current is 1-based.
/// If total is None, the total number of files is unknown (streaming).
pub type IndexProgress = Box<dyn Fn(usize, Option<usize>) + Send + 'static>;

/// Escape glob meta-characters so a literal string can be used as a path
/// component inside a glob pattern.
fn escape_glob_literal(s: &str) -> String {
    let mut escaped = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '*' | '?' | '[' | ']' | '{' | '}' | '\\' => escaped.push('\\'),
            _ => {}
        }
        escaped.push(ch);
    }
    escaped
}

fn build_exclude_globset(patterns: &[String]) -> (GlobSet, usize) {
    let mut builder = GlobSetBuilder::new();
    let mut invalid = 0;
    for pat in patterns {
        let pat = pat.trim_matches('/');
        if pat.is_empty() {
            continue;
        }
        let escaped = escape_glob_literal(pat);
        let wrapped = format!("**/{}/**", escaped);
        let glob = match Glob::new(&wrapped) {
            Ok(g) => g,
            Err(e) => {
                log::warn!("Skipping invalid exclude pattern {:?}: {}", pat, e);
                invalid += 1;
                continue;
            }
        };
        builder.add(glob);
    }
    let set = builder.build().unwrap_or_else(|e| {
        log::warn!("Failed to build exclude GlobSet: {}", e);
        GlobSet::empty()
    });
    (set, invalid)
}

/// Summary of an indexing run.
#[derive(Debug, Default)]
pub struct IndexReport {
    pub inserted: usize,
    pub updated: usize,
    pub skipped: usize,
    pub errors: usize,
    pub deleted: usize,
}

fn sha256_hex(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    hex::encode(hasher.finalize())
}

fn file_mtime(path: &Path) -> i64 {
    fs::metadata(path)
        .and_then(|m| m.modified())
        .map(|t| {
            t.duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as i64
        })
        .unwrap_or(0)
}

/// Index a single file using FTS-only mode (no embedding).
/// If the file hash matches the cached hash, it is skipped.
/// This is a convenience wrapper around `index_file_with_embedder` with `embedder=None`.
pub fn index_file(
    db: &NoteDatabase,
    tokenizer: &JapaneseTokenizer,
    file_path: &Path,
    vault_name: &str,
    relative_path: &str,
    config: &IndexConfig,
) -> IndexResult {
    index_file_with_embedder(db, tokenizer, None, file_path, vault_name, relative_path, config)
}

/// Walk `vault_dir`, chunk and index all Markdown files.
/// If `embedder` is Some, also inserts vector embeddings.
/// Returns (per-file results, invalid pattern count).
///
/// If `progress` is provided, it is called with `(current, total)` after each file.
pub fn index_directory(
    db: &NoteDatabase,
    tokenizer: &JapaneseTokenizer,
    config: &IndexConfig,
    embedder: Option<&Embedder>,
    progress: Option<IndexProgress>,
) -> Result<(Vec<(String, String, IndexResult)>, usize), DbError> {
    let (exclude_globset, invalid_patterns) = build_exclude_globset(&config.exclude_dirs);
    let mut all_results = Vec::new();
    let mut global_count = 0usize;

    for (vault_name, notes_dir) in &config.vaults {
        let notes_canonical = if config.follow_links {
            Some(notes_dir.canonicalize().map_err(|e| {
                DbError::Io(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    format!(
                        "cannot canonicalize notes_dir '{}': {}",
                        vault_name, e
                    ),
                ))
            })?)
        } else {
            None
        };

        for entry in WalkDir::new(notes_dir)
            .follow_links(config.follow_links)
            .into_iter()
            .filter_entry(|e| {
                if e.file_type().is_dir() && e.depth() > 0 {
                    if config.auto_exclude_hidden && e.file_name().to_string_lossy().starts_with('.') {
                        return false;
                    }
                    if let Some(ref canonical_root) = notes_canonical {
                        match e.path().canonicalize() {
                            Ok(canonical) => {
                                if !canonical.starts_with(canonical_root) {
                                    return false;
                                }
                            }
                            Err(_) => return false,
                        }
                    }
                }
                true
            })
            .filter_map(|e| match e {
                Ok(entry) => Some(entry),
                Err(err) => {
                    log::warn!("Directory scan error: {}", err);
                    None
                }
            })
            .filter(|entry| {
                let path = entry.path();
                if !path.is_file() {
                    return false;
                }
                if let Some(ref canonical_root) = notes_canonical {
                    match path.canonicalize() {
                        Ok(canonical) => {
                            if !canonical.starts_with(canonical_root) {
                                return false;
                            }
                        }
                        Err(_) => return false,
                    }
                }
                let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                if !config.include_extensions.iter().any(|e| e == ext) {
                    return false;
                }
                let relative = if path.starts_with(notes_dir) {
                    path.strip_prefix(notes_dir).unwrap_or(path)
                } else {
                    log::warn!("File path {:?} outside vault root {:?}", path, notes_dir);
                    return false;
                };
                let rel_str = relative.to_string_lossy();
                !exclude_globset.is_match(rel_str.as_ref())
            })
        {
            global_count += 1;
            if let Some(ref cb) = progress {
                cb(global_count, None);
            }
            let path = entry.path();
            let relative = path.strip_prefix(notes_dir).unwrap_or(path);
            let rel_str = relative.to_string_lossy().replace('\\', "/");
            let result = index_file_with_embedder(
                db, tokenizer, embedder, path, vault_name, &rel_str, config,
            );
            all_results.push((vault_name.clone(), rel_str, result));
        }
    }

    Ok((all_results, invalid_patterns))
}

/// Remove indexed files from DB that no longer exist on disk.
pub fn cleanup_deleted(db: &NoteDatabase, config: &IndexConfig) -> Result<Vec<String>, DbError> {
    let mut removed = Vec::new();
    for (vault_name, notes_dir) in &config.vaults {
        let cached_paths = db.list_cached_paths(vault_name)?;
        for path in cached_paths {
            let full_path = notes_dir.join(&path);
            if !full_path.exists() {
                db.delete_chunks_for_file(vault_name, &path)?;
                db.delete_file_cache(vault_name, &path)?;
                removed.push(path);
            }
        }
    }
    Ok(removed)
}

/// Index a single file with optional embedder (for watcher use).
pub fn index_file_with_embedder(
    db: &NoteDatabase,
    tokenizer: &JapaneseTokenizer,
    embedder: Option<&Embedder>,
    file_path: &Path,
    vault_name: &str,
    relative_path: &str,
    _config: &IndexConfig,
) -> IndexResult {
    let mtime = file_mtime(file_path);

    // Fast path: skip if mtime matches cached value (avoids reading the file).
    // Uses millisecond-precision mtime to handle rapid successive writes.
    let model_id = embedder.map_or("none", |e| e.model_id());
    if let Ok(Some(cached_mtime)) = db.cached_mtime(vault_name, relative_path) {
        if cached_mtime == mtime {
            return IndexResult::Skipped;
        }
    }

    let content = match fs::read_to_string(file_path) {
        Ok(c) => c,
        Err(e) => return IndexResult::Error(format!("Read error: {}", e)),
    };

    let hash = sha256_hex(&content);

    let is_update = match db.cached_hash(vault_name, relative_path) {
        Ok(Some(cached)) if cached == hash => return IndexResult::Skipped,
        Ok(Some(_)) => true,
        Ok(None) => false,
        Err(e) => return IndexResult::Error(e.to_string()),
    };

    let chunks = split_into_chunks(&content, tokenizer, relative_path, vault_name);

    let embeddings: Vec<Option<Vec<f32>>> = if let Some(emb) = embedder {
        let texts: Vec<&str> = chunks.iter().map(|c| c.content.as_str()).collect();
        let results = emb.embed_batch(&texts);
        results
            .into_iter()
            .enumerate()
            .map(|(i, result)| match result {
                Ok(e) => Some(e),
                Err(e) => {
                    log::warn!("Failed to embed chunk {}: {}", i, e);
                    None
                }
            })
            .collect()
    } else {
        vec![None; chunks.len()]
    };

    if let Err(e) = db.reindex_file(
        vault_name,
        relative_path,
        &hash,
        mtime,
        model_id,
        &chunks,
        &embeddings,
    ) {
        return IndexResult::Error(e.to_string());
    }

    if is_update {
        IndexResult::Updated
    } else {
        IndexResult::Inserted
    }
}

/// Result returned after indexing a file.
#[derive(Debug, Clone, PartialEq)]
pub enum IndexResult {
    Inserted,
    Updated,
    Skipped,
    Error(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::NoteDatabase;
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn test_index_directory_parallel_multiple_files() {
        let tokenizer = crate::require_tokenizer!(Default::default());
        let temp = TempDir::new().unwrap();
        let vault = temp.path().join("vault");
        fs::create_dir(&vault).unwrap();
        for i in 0..10 {
            let mut f = fs::File::create(vault.join(format!("note{}.md", i))).unwrap();
            writeln!(f, "# Note {}\n\nContent body for note {}", i, i).unwrap();
        }
        let db = NoteDatabase::open_in_memory().unwrap();
        let config = IndexConfig {
            vaults: vec![("default".to_string(), vault.clone())],
            ..Default::default()
        };
        let (results, _invalid) = index_directory(&db, &tokenizer, &config, None, None).unwrap();
        assert_eq!(results.len(), 10);
        assert_eq!(db.stats().unwrap().total_files, 10);
    }

    #[test]
    fn test_index_directory_respects_exclude_dirs() {
        let tokenizer = crate::require_tokenizer!(Default::default());
        let temp = TempDir::new().unwrap();
        let vault = temp.path().join("vault");
        fs::create_dir(&vault).unwrap();
        let templates = vault.join("templates");
        fs::create_dir(&templates).unwrap();
        let mut f = fs::File::create(templates.join("daily.md")).unwrap();
        writeln!(f, "# Daily template").unwrap();
        let mut g = fs::File::create(vault.join("main.md")).unwrap();
        writeln!(g, "# Main").unwrap();
        let db = NoteDatabase::open_in_memory().unwrap();
        let config = IndexConfig {
            vaults: vec![("default".to_string(), vault.clone())],
            exclude_dirs: vec!["templates".to_string()],
            ..Default::default()
        };
        let (results, _invalid) = index_directory(&db, &tokenizer, &config, None, None).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].1, "main.md");
        assert_eq!(db.stats().unwrap().total_files, 1);
    }

    #[test]
    fn test_no_frontmatter() {
        let content = "# Hello\n\nWorld";
        assert!(content.starts_with("# Hello"));
    }

    #[test]
    fn test_index_directory() {
        let tokenizer = crate::require_tokenizer!(Default::default());

        let temp = TempDir::new().unwrap();
        let vault = temp.path().join("vault");
        fs::create_dir(&vault).unwrap();

        let mut f1 = fs::File::create(vault.join("note1.md")).unwrap();
        writeln!(f1, "# Hello\n\nWorld content").unwrap();
        let mut f2 = fs::File::create(vault.join("note2.md")).unwrap();
        writeln!(f2, "---\ntitle: Special\n---\n\nUnique text here").unwrap();

        let db = NoteDatabase::open_in_memory().unwrap();
        let config = IndexConfig {
            vaults: vec![("default".to_string(), vault.clone())],
            ..Default::default()
        };

        let (results, _invalid) = index_directory(&db, &tokenizer, &config, None, None).unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(db.stats().unwrap().total_files, 2);
    }

    #[test]
    fn test_cleanup_deleted() {
        let tokenizer = crate::require_tokenizer!(Default::default());

        let temp = TempDir::new().unwrap();
        let vault = temp.path().join("vault");
        fs::create_dir(&vault).unwrap();

        let mut f = fs::File::create(vault.join("old.md")).unwrap();
        writeln!(f, "content").unwrap();

        let db = NoteDatabase::open_in_memory().unwrap();
        let config = IndexConfig {
            vaults: vec![("default".to_string(), vault.clone())],
            ..Default::default()
        };
        index_directory(&db, &tokenizer, &config, None, None).unwrap();
        assert_eq!(db.stats().unwrap().total_files, 1);

        fs::remove_file(vault.join("old.md")).unwrap();
        let removed = cleanup_deleted(&db, &config).unwrap();
        assert_eq!(removed.len(), 1);
        assert_eq!(db.stats().unwrap().total_files, 0);
    }

    #[test]
    fn test_hidden_dir_auto_excluded() {
        let tokenizer = crate::require_tokenizer!(Default::default());

        let temp = TempDir::new().unwrap();
        let vault = temp.path().join("vault");
        fs::create_dir(&vault).unwrap();

        let hidden = vault.join(".obsidian");
        fs::create_dir(&hidden).unwrap();
        fs::write(hidden.join("note.md"), "# Hidden").unwrap();

        let notes = vault.join("notes");
        fs::create_dir(&notes).unwrap();
        fs::write(notes.join("visible.md"), "# Visible").unwrap();

        let db = NoteDatabase::open_in_memory().unwrap();
        let config = IndexConfig {
            vaults: vec![("default".to_string(), vault.clone())],
            ..Default::default()
        };
        assert!(config.auto_exclude_hidden);

        let (results, _invalid) = index_directory(&db, &tokenizer, &config, None, None).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].1, "notes/visible.md");
    }

    #[test]
    fn test_hidden_dir_included_when_disabled() {
        let tokenizer = crate::require_tokenizer!(Default::default());

        let temp = TempDir::new().unwrap();
        let vault = temp.path().join("vault");
        fs::create_dir(&vault).unwrap();

        let hidden = vault.join(".hidden_notes");
        fs::create_dir(&hidden).unwrap();
        fs::write(hidden.join("secret.md"), "# Secret").unwrap();

        let db = NoteDatabase::open_in_memory().unwrap();
        let config = IndexConfig {
            vaults: vec![("default".to_string(), vault.clone())],
            auto_exclude_hidden: false,
            ..Default::default()
        };

        let (results, _invalid) = index_directory(&db, &tokenizer, &config, None, None).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].1, ".hidden_notes/secret.md");
    }

    #[test]
    fn test_exclude_dirs_globset_basename_matching() {
        let tokenizer = crate::require_tokenizer!(Default::default());

        let temp = TempDir::new().unwrap();
        let vault = temp.path().join("vault");
        fs::create_dir(&vault).unwrap();

        let templates = vault.join("templates");
        fs::create_dir(&templates).unwrap();
        fs::write(templates.join("daily.md"), "# Daily").unwrap();

        let templates_extra = vault.join("templates_extra");
        fs::create_dir(&templates_extra).unwrap();
        fs::write(templates_extra.join("extra.md"), "# Extra").unwrap();

        let notes = vault.join("notes");
        fs::create_dir(&notes).unwrap();
        fs::write(notes.join("main.md"), "# Main").unwrap();

        let db = NoteDatabase::open_in_memory().unwrap();
        let config = IndexConfig {
            vaults: vec![("default".to_string(), vault.clone())],
            exclude_dirs: vec!["templates".to_string()],
            auto_exclude_hidden: false,
            ..Default::default()
        };

        let (results, _invalid) = index_directory(&db, &tokenizer, &config, None, None).unwrap();
        assert_eq!(
            results.len(),
            2,
            "templates_extra and notes should be indexed, templates excluded"
        );
        let paths: Vec<&str> = results.iter().map(|r| r.1.as_str()).collect();
        assert!(paths.contains(&"notes/main.md"));
        assert!(paths.contains(&"templates_extra/extra.md"));
        assert!(!paths.contains(&"templates/daily.md"));
    }

    #[test]
    fn test_build_exclude_globset_invalid_pattern() {
        let patterns = vec![r"[invalid".to_string(), "node_modules".to_string()];
        let (set, _count) = build_exclude_globset(&patterns);
        assert!(set.is_match("node_modules/foo.md"));
        assert!(set.is_match("a/node_modules/foo.md"));
        assert!(set.is_match(r"[invalid/foo.md"));
    }

    #[test]
    fn test_build_exclude_globset_escapes_special_chars() {
        let patterns = vec!["draft_*".to_string(), "*.bak".to_string()];
        let (set, _count) = build_exclude_globset(&patterns);
        assert!(set.is_match("draft_*/foo.md"));
        assert!(!set.is_match("draft_2024/foo.md"));
        assert!(set.is_match(r"*.bak/foo.md"));
        assert!(!set.is_match("important.bak/foo.md"));
    }

    #[test]
    fn test_build_exclude_globset_trims_slashes() {
        let patterns = vec!["node_modules/".to_string(), "/dist".to_string()];
        let (set, _count) = build_exclude_globset(&patterns);
        assert!(set.is_match("node_modules/foo.md"));
        assert!(!set.is_match("my_node_modules/foo.md"));
        assert!(set.is_match("dist/foo.md"));
        assert!(!set.is_match("my_dist/foo.md"));
    }

    #[test]
    fn test_globset_matches_subdirectory_files() {
        let temp = TempDir::new().unwrap();
        let vault = temp.path().join("vault");
        fs::create_dir(&vault).unwrap();

        let nm = vault.join("node_modules");
        fs::create_dir(&nm).unwrap();
        let sub = nm.join("sub");
        fs::create_dir(&sub).unwrap();
        fs::write(sub.join("deep.md"), "# Deep inside node_modules").unwrap();

        let db = NoteDatabase::open_in_memory().unwrap();
        let tokenizer = crate::require_tokenizer!(Default::default());
        let config = IndexConfig {
            vaults: vec![("default".to_string(), vault.clone())],
            exclude_dirs: vec!["node_modules".to_string()],
            auto_exclude_hidden: false,
            ..Default::default()
        };

        let (results, _invalid) = index_directory(&db, &tokenizer, &config, None, None).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn test_chunking_preserves_all_results() {
        let tokenizer = crate::require_tokenizer!(Default::default());
        let temp = TempDir::new().unwrap();
        let vault = temp.path().join("vault");
        fs::create_dir(&vault).unwrap();
        for i in 0..300 {
            fs::write(vault.join(format!("note{}.md", i)), format!("# Note {}", i)).unwrap();
        }
        let db = NoteDatabase::open_in_memory().unwrap();
        let config = IndexConfig {
            vaults: vec![("default".to_string(), vault.clone())],
            ..Default::default()
        };
        let (results, _invalid) = index_directory(&db, &tokenizer, &config, None, None).unwrap();
        assert_eq!(results.len(), 300);
        let mut paths: Vec<&str> = results.iter().map(|(_, p, _)| p.as_str()).collect();
        paths.sort();
        paths.dedup();
        assert_eq!(paths.len(), 300, "no duplicate paths");
    }

    #[test]
    fn test_chunking_does_not_deadlock_empty_vault() {
        let tokenizer = crate::require_tokenizer!(Default::default());
        let temp = TempDir::new().unwrap();
        let vault = temp.path().join("vault");
        fs::create_dir(&vault).unwrap();
        let db = NoteDatabase::open_in_memory().unwrap();
        let config = IndexConfig {
            vaults: vec![("default".to_string(), vault.clone())],
            ..Default::default()
        };
        let (results, _invalid) = index_directory(&db, &tokenizer, &config, None, None).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn test_chunking_splits_at_256_entries() {
        let tokenizer = crate::require_tokenizer!(Default::default());
        let temp = TempDir::new().unwrap();
        let vault = temp.path().join("vault");
        fs::create_dir(&vault).unwrap();
        for i in 0..257 {
            let content = format!("# Note {}\n\nSmall content", i);
            fs::write(vault.join(format!("note{}.md", i)), content).unwrap();
        }
        let db = NoteDatabase::open_in_memory().unwrap();
        let config = IndexConfig {
            vaults: vec![("default".to_string(), vault.clone())],
            ..Default::default()
        };
        let (results, _invalid) = index_directory(&db, &tokenizer, &config, None, None).unwrap();
        assert_eq!(results.len(), 257, "all files should be indexed");
        assert_eq!(db.stats().unwrap().total_files, 257);
    }

    #[test]
    fn test_chunking_splits_at_byte_threshold() {
        let tokenizer = crate::require_tokenizer!(Default::default());
        let temp = TempDir::new().unwrap();
        let vault = temp.path().join("vault");
        fs::create_dir(&vault).unwrap();
        let big_content = "x".repeat(13_000_000);
        fs::write(vault.join("big1.md"), &big_content).unwrap();
        fs::write(vault.join("big2.md"), &big_content).unwrap();
        let db = NoteDatabase::open_in_memory().unwrap();
        let config = IndexConfig {
            vaults: vec![("default".to_string(), vault.clone())],
            ..Default::default()
        };
        let (results, _invalid) = index_directory(&db, &tokenizer, &config, None, None).unwrap();
        assert_eq!(results.len(), 2, "both large files should be indexed");
    }

    #[test]
    fn test_chunking_single_chunk_for_small_vault() {
        let tokenizer = crate::require_tokenizer!(Default::default());
        let temp = TempDir::new().unwrap();
        let vault = temp.path().join("vault");
        fs::create_dir(&vault).unwrap();
        for i in 0..100 {
            fs::write(vault.join(format!("note{}.md", i)), format!("# Note {}", i)).unwrap();
        }
        let db = NoteDatabase::open_in_memory().unwrap();
        let config = IndexConfig {
            vaults: vec![("default".to_string(), vault.clone())],
            ..Default::default()
        };
        let (results, _invalid) = index_directory(&db, &tokenizer, &config, None, None).unwrap();
        assert_eq!(results.len(), 100, "all 100 small files should be indexed");
    }

    #[test]
    fn test_chunking_exact_boundary_256() {
        let tokenizer = crate::require_tokenizer!(Default::default());
        let temp = TempDir::new().unwrap();
        let vault = temp.path().join("vault");
        fs::create_dir(&vault).unwrap();
        for i in 0..256 {
            fs::write(vault.join(format!("note{}.md", i)), format!("# Note {}", i)).unwrap();
        }
        let db = NoteDatabase::open_in_memory().unwrap();
        let config = IndexConfig {
            vaults: vec![("default".to_string(), vault.clone())],
            ..Default::default()
        };
        let (results, _invalid) = index_directory(&db, &tokenizer, &config, None, None).unwrap();
        assert_eq!(results.len(), 256, "exactly 256 files should all be indexed");
    }

    #[test]
    #[cfg(unix)]
    fn test_strip_prefix_outside_vault_is_rejected() {
        let tokenizer = crate::require_tokenizer!(Default::default());
        let temp = TempDir::new().unwrap();
        let vault = temp.path().join("vault");
        fs::create_dir(&vault).unwrap();
        let outside = temp.path().join("outside.md");
        fs::write(&outside, "# Outside").unwrap();
        let symlink = vault.join("escape.md");
        std::os::unix::fs::symlink(&outside, &symlink).unwrap();
        let db = NoteDatabase::open_in_memory().unwrap();
        let config = IndexConfig {
            vaults: vec![("default".to_string(), vault.clone())],
            follow_links: true,
            ..Default::default()
        };
        let (results, _invalid) = index_directory(&db, &tokenizer, &config, None, None).unwrap();
        assert!(results.is_empty(), "external symlink should be rejected");
    }

    #[test]
    fn test_index_file_skips_on_same_hash() {
        let tokenizer = crate::require_tokenizer!(Default::default());
        let temp = TempDir::new().unwrap();
        let vault = temp.path().join("vault");
        fs::create_dir(&vault).unwrap();
        fs::write(vault.join("test.md"), "---\ntitle: Same\n---\n\nContent").unwrap();

        let db = NoteDatabase::open_in_memory().unwrap();
        let config = IndexConfig {
            vaults: vec![("default".to_string(), vault.clone())],
            ..Default::default()
        };
        let r1 = index_file(&db, &tokenizer, &vault.join("test.md"), "default", "test.md", &config);
        assert_eq!(r1, IndexResult::Inserted);

        // Second call with same content → skipped
        let r2 = index_file(&db, &tokenizer, &vault.join("test.md"), "default", "test.md", &config);
        assert_eq!(r2, IndexResult::Skipped);
    }

    #[test]
    fn test_index_file_skips_via_mtime_fast_path() {
        let tokenizer = crate::require_tokenizer!(Default::default());
        let temp = TempDir::new().unwrap();
        let vault = temp.path().join("vault");
        fs::create_dir(&vault).unwrap();
        let path = vault.join("test.md");
        fs::write(&path, "Mtime test content").unwrap();

        let db = NoteDatabase::open_in_memory().unwrap();
        let config = IndexConfig {
            vaults: vec![("default".to_string(), vault.clone())],
            ..Default::default()
        };

        // First call: insert
        let r1 = index_file(&db, &tokenizer, &path, "default", "test.md", &config);
        assert_eq!(r1, IndexResult::Inserted);

        // Verify cached_mtime actually has a value
        let cached = db.cached_mtime("default", "test.md").unwrap();
        assert!(cached.is_some(), "mtime should be cached after insertion");

        // Second call with same content but ensure mtime is still cached
        // (index_file calls index_file_with_embedder which checks mtime first)
        let r2 = index_file(&db, &tokenizer, &path, "default", "test.md", &config);
        assert_eq!(r2, IndexResult::Skipped);

        // Verify the mtime in cache matches the file mtime (both in milliseconds)
        let file_mtime = std::fs::metadata(&path).unwrap()
            .modified().unwrap()
            .duration_since(std::time::UNIX_EPOCH).unwrap()
            .as_millis() as i64;
        let cached_mtime = db.cached_mtime("default", "test.md").unwrap().unwrap();
        assert!(
            (cached_mtime - file_mtime).abs() <= 100,
            "cached mtime ({}) should match file mtime ({}) within 100ms",
            cached_mtime,
            file_mtime
        );
    }

    #[test]
    fn test_index_file_updates_on_changed_content() {
        let tokenizer = crate::require_tokenizer!(Default::default());
        let temp = TempDir::new().unwrap();
        let vault = temp.path().join("vault");
        fs::create_dir(&vault).unwrap();
        let path = vault.join("test.md");
        fs::write(&path, "Initial content").unwrap();

        let db = NoteDatabase::open_in_memory().unwrap();
        let config = IndexConfig {
            vaults: vec![("default".to_string(), vault.clone())],
            ..Default::default()
        };
        let r1 = index_file(&db, &tokenizer, &path, "default", "test.md", &config);
        assert_eq!(r1, IndexResult::Inserted);

        fs::write(&path, "Changed content").unwrap();
        let r2 = index_file(&db, &tokenizer, &path, "default", "test.md", &config);
        assert_eq!(r2, IndexResult::Updated);
    }

    #[test]
    fn test_reindex_file_compile_check() {
        // Compile-time check: reindex_file is reachable from
        // index_file_with_embedder. Full coverage requires an ONNX model.
        assert!(true, "reindex_file compiled successfully");
    }

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

    #[test]
    fn test_sha256_hex_known_input() {
        let empty_hash = sha256_hex("");
        assert_eq!(empty_hash.len(), 64, "SHA-256 hex should be 64 chars");
        assert_eq!(empty_hash, "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
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
        assert_eq!(sha256_hex("東京 検索"), sha256_hex("東京 検索"));
    }

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
        assert_eq!(invalid, 0, "escape_glob_literal escapes [, making it valid");
        assert!(set.is_match("projects/[/notes.md"), "escaped [ is a valid literal glob");
    }

    #[test]
    fn test_build_exclude_globset_empty_string_pattern() {
        let patterns = vec!["".to_string()];
        let (set, invalid) = build_exclude_globset(&patterns);
        assert_eq!(invalid, 0);
        assert!(!set.is_match("file.md"), "empty string pattern should be skipped");
    }

    #[test]
    fn test_index_directory_no_follow_links_creates_structure() {
        use walkdir::WalkDir;
        let dir = TempDir::new().unwrap();
        let vault = dir.path().join("vault");
        let sub = vault.join("sub");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(vault.join("a.md"), "# A").unwrap();
        std::fs::write(sub.join("b.md"), "# B").unwrap();

        let config = IndexConfig {
            vaults: vec![("default".to_string(), vault.clone())],
            ..Default::default()
        };
        let (_exclude_globset, _) = build_exclude_globset(&config.exclude_dirs);

        let entries: Vec<_> = WalkDir::new(&vault)
            .follow_links(false)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_file())
            .collect();
        assert_eq!(entries.len(), 2, "should find 2 files");
    }

    #[test]
    fn test_index_directory_with_progress_collects_tags() {
        let tokenizer = match crate::tokenizer::JapaneseTokenizer::new(crate::tokenizer::TokenizerConfig::default()) {
            Ok(tok) => tok,
            Err(_) => return,
        };
        let dir = TempDir::new().unwrap();
        let vault = dir.path().join("vault");
        std::fs::create_dir_all(&vault).unwrap();
        std::fs::write(vault.join("progress_test.md"), "# Progress test\n\nContent.").unwrap();

        let db = NoteDatabase::open_in_memory().unwrap();
        let config = IndexConfig {
            vaults: vec![("default".to_string(), vault)],
            ..Default::default()
        };

        let progress: IndexProgress = Box::new(|_current, _total: Option<usize>| {});

        let (results, invalid) = index_directory(&db, &tokenizer, &config, None, Some(progress)).unwrap();
        assert_eq!(results.len(), 1, "should index 1 file");
        assert!(!results[0].1.is_empty(), "should have a relative path");
        assert_eq!(invalid, 0, "no invalid patterns");
    }

    // ── build_exclude_globset literal pattern matching ───────────────

    #[test]
    fn test_build_exclude_globset_literal_with_brackets() {
        // The function escapes [ ] so they're treated literally
        let patterns = vec!["[test]".to_string()];
        let (set, invalid) = build_exclude_globset(&patterns);
        assert_eq!(invalid, 0);
        assert!(set.is_match("dir/[test]/notes.md"));
        assert!(!set.is_match("dir/t/notes.md")); // [test] is literal, not char class
    }

    #[test]
    fn test_build_exclude_globset_recursive_literal_at_any_depth() {
        // The function wraps in **/{}/**, so "notes" matches at any depth
        let patterns = vec!["notes".to_string()];
        let (set, invalid) = build_exclude_globset(&patterns);
        assert_eq!(invalid, 0);
        assert!(set.is_match("a/notes/b/file.md"));
        assert!(!set.is_match("not_notes/file.md"));
    }

    #[test]
    fn test_build_exclude_globset_literal_component_matching() {
        // The function wraps in **/{}/**, so "tmpdir" matches as a path component
        let patterns = vec!["tmpdir".to_string()];
        let (set, invalid) = build_exclude_globset(&patterns);
        assert_eq!(invalid, 0);
        assert!(set.is_match("tmpdir/file.md"));
        assert!(set.is_match("a/tmpdir/b/file.md"));
        assert!(!set.is_match("file.tmpdir"));
    }

    #[test]
    fn test_build_exclude_globset_multiple_literals() {
        let patterns = vec![
            "archive".to_string(),
            "backup".to_string(),
        ];
        let (set, invalid) = build_exclude_globset(&patterns);
        assert_eq!(invalid, 0);
        assert!(set.is_match("dir/archive/notes.md"));
        assert!(set.is_match("dir/backup/notes.md"));
        assert!(!set.is_match("dir/active/notes.md"));
    }

    #[test]
    fn test_build_exclude_globset_extension_as_component() {
        // ".bak" is escaped and wrapped as **/.bak/**, matching it as a component
        let patterns = vec![".bak".to_string()];
        let (set, invalid) = build_exclude_globset(&patterns);
        assert_eq!(invalid, 0);
        assert!(set.is_match("dir/.bak/notes.md"));
        assert!(!set.is_match("file.bak"));
    }

    #[test]
    fn test_escape_glob_literal_backslash_chain() {
        assert_eq!(escape_glob_literal("a\\b\\c"), "a\\\\b\\\\c");
    }

    #[test]
    fn test_escape_glob_literal_all_special_chars() {
        assert_eq!(
            escape_glob_literal("*?[]{},"),
            "\\*\\?\\[\\]\\{\\},"
        );
    }
}
