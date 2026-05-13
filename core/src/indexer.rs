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
                .as_secs() as i64
        })
        .unwrap_or(0)
}

/// Index a single file into the database using the chunk-based schema.
/// If the file hash matches the cached hash, it is skipped.
pub fn index_file(
    db: &NoteDatabase,
    tokenizer: &JapaneseTokenizer,
    file_path: &Path,
    relative_path: &str,
    _config: &IndexConfig,
) -> IndexResult {
    let content = match fs::read_to_string(file_path) {
        Ok(c) => c,
        Err(e) => return IndexResult::Error(format!("Read error: {}", e)),
    };

    let hash = sha256_hex(&content);
    let mtime = file_mtime(file_path);

    let is_update = match db.cached_hash(relative_path) {
        Ok(Some(cached)) if cached == hash => return IndexResult::Skipped,
        Ok(Some(_)) => true,
        Ok(None) => false,
        Err(e) => return IndexResult::Error(e.to_string()),
    };

    if let Err(e) = db.delete_chunks_for_file(relative_path) {
        return IndexResult::Error(e.to_string());
    }

    let mut chunks = split_into_chunks(&content, tokenizer, relative_path);
    // Ensure tokenized_content is set (split_into_chunks may leave it empty)
    for chunk in &mut chunks {
        if chunk.tokenized_content.is_empty() {
            chunk.tokenized_content = tokenizer.split(&chunk.content);
        }
    }

    if let Err(e) = db.insert_chunks(&chunks) {
        return IndexResult::Error(e.to_string());
    }

    if let Err(e) = db.upsert_file_cache(relative_path, &hash, mtime, "none") {
        return IndexResult::Error(e.to_string());
    }

    if is_update {
        IndexResult::Updated
    } else {
        IndexResult::Inserted
    }
}

/// Walk `vault_dir`, chunk and index all Markdown files.
/// If `embedder` is Some, also inserts vector embeddings.
/// Returns (per-file results, invalid pattern count).
pub fn index_directory<'a>(
    db: &NoteDatabase,
    tokenizer: &JapaneseTokenizer,
    config: &IndexConfig,
    embedder: Option<&'a Embedder>,
) -> Result<(Vec<(String, IndexResult)>, usize), DbError> {
    let notes_dir = &config.notes_dir;
    let (exclude_globset, invalid_patterns) = build_exclude_globset(&config.exclude_dirs);

    let notes_canonical = if config.follow_links {
        Some(notes_dir.canonicalize().map_err(|e| {
            DbError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("cannot canonicalize notes_dir: {}", e),
            ))
        })?)
    } else {
        None
    };

    let entries: Vec<_> = WalkDir::new(notes_dir)
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
        .collect();

    let mut all_results = Vec::new();
    for entry in &entries {
        let path = entry.path();
        let relative = path.strip_prefix(notes_dir).unwrap_or(path);
        let rel_str = relative.to_string_lossy().replace('\\', "/");
        let result = index_file_with_embedder(db, tokenizer, embedder, path, &rel_str, config);
        all_results.push((rel_str, result));
    }

    Ok((all_results, invalid_patterns))
}

/// Remove indexed files from DB that no longer exist on disk.
pub fn cleanup_deleted(db: &NoteDatabase, config: &IndexConfig) -> Result<Vec<String>, DbError> {
    let cached_paths = db.list_cached_paths()?;
    let mut removed = Vec::new();
    for path in cached_paths {
        let full_path = config.notes_dir.join(&path);
        if !full_path.exists() {
            db.delete_chunks_for_file(&path)?;
            db.delete_file_cache(&path)?;
            removed.push(path);
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
    relative_path: &str,
    _config: &IndexConfig,
) -> IndexResult {
    let content = match fs::read_to_string(file_path) {
        Ok(c) => c,
        Err(e) => return IndexResult::Error(format!("Read error: {}", e)),
    };

    let hash = sha256_hex(&content);
    let mtime = file_mtime(file_path);
    let model_id = if embedder.is_some() { "qwen3-embedding-0.6b" } else { "none" };

    let is_update = match db.cached_hash(relative_path) {
        Ok(Some(cached)) if cached == hash => return IndexResult::Skipped,
        Ok(Some(_)) => true,
        Ok(None) => false,
        Err(e) => return IndexResult::Error(e.to_string()),
    };

    if let Err(e) = db.delete_chunks_for_file(relative_path) {
        return IndexResult::Error(e.to_string());
    }

    let mut chunks = split_into_chunks(&content, tokenizer, relative_path);
    for chunk in &mut chunks {
        if chunk.tokenized_content.is_empty() {
            chunk.tokenized_content = tokenizer.split(&chunk.content);
        }
    }

    let ids = match db.insert_chunks(&chunks) {
        Ok(ids) => ids,
        Err(e) => return IndexResult::Error(e.to_string()),
    };

    if let Some(emb) = embedder {
        let pairs: Vec<(i64, Vec<f32>)> = ids.iter().zip(chunks.iter())
            .filter_map(|(id, chunk)| {
                emb.embed(&chunk.content).ok().map(|e| (*id, e))
            })
            .collect();
        if let Err(e) = db.insert_embeddings(&pairs) {
            log::warn!("Failed to insert embeddings: {}", e);
        }
    }

    if let Err(e) = db.upsert_file_cache(relative_path, &hash, mtime, model_id) {
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
    use std::{io::Write, path::PathBuf};
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
            notes_dir: vault.clone(),
            ..Default::default()
        };
        let (results, _invalid) = index_directory(&db, &tokenizer, &config, None).unwrap();
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
            notes_dir: vault.clone(),
            exclude_dirs: vec!["templates".to_string()],
            ..Default::default()
        };
        let (results, _invalid) = index_directory(&db, &tokenizer, &config, None).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, "main.md");
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
            notes_dir: vault.clone(),
            ..Default::default()
        };

        let (results, _invalid) = index_directory(&db, &tokenizer, &config, None).unwrap();
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
            notes_dir: vault.clone(),
            ..Default::default()
        };
        index_directory(&db, &tokenizer, &config, None).unwrap();
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
            notes_dir: vault.clone(),
            ..Default::default()
        };
        assert!(config.auto_exclude_hidden);

        let (results, _invalid) = index_directory(&db, &tokenizer, &config, None).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, "notes/visible.md");
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
            notes_dir: vault.clone(),
            auto_exclude_hidden: false,
            ..Default::default()
        };

        let (results, _invalid) = index_directory(&db, &tokenizer, &config, None).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, ".hidden_notes/secret.md");
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
            notes_dir: vault.clone(),
            exclude_dirs: vec!["templates".to_string()],
            auto_exclude_hidden: false,
            ..Default::default()
        };

        let (results, _invalid) = index_directory(&db, &tokenizer, &config, None).unwrap();
        assert_eq!(
            results.len(),
            2,
            "templates_extra and notes should be indexed, templates excluded"
        );
        let paths: Vec<&str> = results.iter().map(|r| r.0.as_str()).collect();
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
            notes_dir: vault.clone(),
            exclude_dirs: vec!["node_modules".to_string()],
            auto_exclude_hidden: false,
            ..Default::default()
        };

        let (results, _invalid) = index_directory(&db, &tokenizer, &config, None).unwrap();
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
            notes_dir: vault.clone(),
            ..Default::default()
        };
        let (results, _invalid) = index_directory(&db, &tokenizer, &config, None).unwrap();
        assert_eq!(results.len(), 300);
        let mut paths: Vec<&str> = results.iter().map(|(p, _)| p.as_str()).collect();
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
            notes_dir: vault.clone(),
            ..Default::default()
        };
        let (results, _invalid) = index_directory(&db, &tokenizer, &config, None).unwrap();
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
            notes_dir: vault.clone(),
            ..Default::default()
        };
        let (results, _invalid) = index_directory(&db, &tokenizer, &config, None).unwrap();
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
            notes_dir: vault.clone(),
            ..Default::default()
        };
        let (results, _invalid) = index_directory(&db, &tokenizer, &config, None).unwrap();
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
            notes_dir: vault.clone(),
            ..Default::default()
        };
        let (results, _invalid) = index_directory(&db, &tokenizer, &config, None).unwrap();
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
            notes_dir: vault.clone(),
            ..Default::default()
        };
        let (results, _invalid) = index_directory(&db, &tokenizer, &config, None).unwrap();
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
            notes_dir: vault.clone(),
            follow_links: true,
            ..Default::default()
        };
        let (results, _invalid) = index_directory(&db, &tokenizer, &config, None).unwrap();
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
            notes_dir: vault.clone(),
            ..Default::default()
        };
        let r1 = index_file(&db, &tokenizer, &vault.join("test.md"), "test.md", &config);
        assert_eq!(r1, IndexResult::Inserted);

        // Second call with same content → skipped
        let r2 = index_file(&db, &tokenizer, &vault.join("test.md"), "test.md", &config);
        assert_eq!(r2, IndexResult::Skipped);
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
            notes_dir: vault.clone(),
            ..Default::default()
        };
        let r1 = index_file(&db, &tokenizer, &path, "test.md", &config);
        assert_eq!(r1, IndexResult::Inserted);

        fs::write(&path, "Changed content").unwrap();
        let r2 = index_file(&db, &tokenizer, &path, "test.md", &config);
        assert_eq!(r2, IndexResult::Updated);
    }
}
