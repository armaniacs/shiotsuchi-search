use crate::{
    chunker::split_into_chunks,
    db::{DbError, NoteDatabase},
    embedder::Embedder,
    models::{IndexConfig, IndexParams, ReindexParams, Task},
    tokenizer::JapaneseTokenizer,
};
use globset::{Glob, GlobSet, GlobSetBuilder};
use sha2::{Digest, Sha256};
use std::{fs, path::{Path, PathBuf}, time::SystemTime};
use walkdir::WalkDir;

/// Optional progress callback for index_directory.
/// Arguments are (current, total) where current is 1-based.
/// If total is None, the total number of files is unknown (streaming).
pub type IndexProgress = Box<dyn Fn(usize, Option<usize>) + Send + 'static>;

/// Escape glob meta-characters so a literal string can be used as a path
/// NOTE: Only backslashes are escaped; glob meta-characters (*, ?, [...])
/// are passed through so users can write patterns like `draft_*` or `*.tmp`.
fn escape_glob_literal(s: &str) -> String {
    let mut escaped = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '\\' => {
                escaped.push_str("\\\\");
            }
            _ => {
                escaped.push(ch);
            }
        }
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
        // Determine how to wrap each pattern:
        // - Patterns containing '/' are partial/full relative path patterns
        //   (prepend `**/` to match anywhere; strip leading `/` if present).
        // - Patterns already ending with `**` are used as-is.
        // - Everything else (bare names like `node_modules`, globs like `*.tmp`)
        //   gets wrapped as `**/{pat}/**` for directory matching.
        let wrapped = if pat.contains('/') || pat.ends_with("**") {
            let stripped = pat.trim_start_matches('/');
            format!("**/{}", stripped)
        } else {
            format!("**/{}/**", escaped)
        };
        let glob = match Glob::new(&wrapped) {
            Ok(g) => g,
            Err(e) => {
                tracing::warn!("Skipping invalid exclude pattern {:?}: {}", pat, e);
                invalid += 1;
                continue;
            }
        };
        builder.add(glob);
    }
    let set = builder.build().unwrap_or_else(|e| {
        tracing::warn!("Failed to build exclude GlobSet: {}", e);
        GlobSet::empty()
    });
    (set, invalid)
}

/// Load patterns from a `.shiotsuchiignore` file at the vault root.
/// Returns an empty vec if the file doesn't exist or can't be read.
pub fn load_shiotsuchiignore(vault_dir: &Path) -> Vec<String> {
    let ignore_file = vault_dir.join(".shiotsuchiignore");
    match std::fs::read_to_string(&ignore_file) {
        Ok(content) => content
            .lines()
            .map(|l| l.trim())
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .map(|l| l.to_string())
            .collect(),
        Err(_) => Vec::new(),
    }
}

/// Check whether a given relative path would be excluded by a set of patterns.
/// Returns `Err(pattern)` with the matching pattern if excluded, or `Ok(())` if not excluded.
pub fn check_ignore(relative_path: &str, patterns: &[String]) -> Result<(), String> {
    for pat in patterns {
        let pat_trimmed = pat.trim_matches('/');
        if pat_trimmed.is_empty() {
            continue;
        }
        let escaped = escape_glob_literal(pat_trimmed);
        let wrapped = if pat_trimmed.contains('/') || pat_trimmed.ends_with("**") {
            let stripped = pat_trimmed.trim_start_matches('/');
            format!("**/{}", stripped)
        } else {
            format!("**/{}/**", escaped)
        };
        if let Ok(glob) = Glob::new(&wrapped) {
            if glob.compile_matcher().is_match(relative_path) {
                return Err(pat.clone());
            }
        }
    }
    Ok(())
}

/// Summary of an indexing run.
#[derive(Debug, Default)]
pub struct IndexReport {
    pub inserted: usize,
    pub updated: usize,
    pub skipped: usize,
    pub errors: usize,
    pub deleted: usize,
    pub excluded: usize,
}

fn sha256_hex(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    hex::encode(hasher.finalize())
}

fn sha256_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
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

/// Extract Obsidian wikilinks from Markdown content.
/// Supports `[[Note Name]]`, `[[Note Name|display text]]`, `[[Note#heading]]`,
/// and `[[Note^blockref]]` formats.
/// Returns the raw link target names (file portion only, without `#` or `^` anchors).
fn extract_wikilinks(content: &str) -> Vec<String> {
    let mut results = Vec::new();
    let bytes = content.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'[' && i + 1 < bytes.len() && bytes[i + 1] == b'[' {
            // Found opening [[
            let after = &content[i + 2..];
            if let Some(end) = after.find("]]") {
                let inner = &after[..end];
                // Split by | to get the link name (before the pipe)
                let mut link_name = inner.split('|').next().unwrap_or(inner).trim();
                // Strip #heading and ^block-reference anchors (common Obsidian patterns)
                if let Some(anchor_pos) = link_name.find('#') {
                    link_name = link_name[..anchor_pos].trim();
                } else if let Some(anchor_pos) = link_name.find('^') {
                    link_name = link_name[..anchor_pos].trim();
                }
                if !link_name.is_empty() {
                    results.push(link_name.to_string());
                }
                i += 2 + end + 2;
                continue;
            }
            i += 2;
            continue;
        }
        i += 1;
    }
    results
}

/// Build a path map for O(1) wikilink resolution from a list of vault file paths.
/// Maps lowercase filename stem (without .md suffix or directory prefix) → shortest
/// matching path. Ambiguous names (same filename in different directories) resolve
/// to the shortest path, matching Obsidian's convention.
pub fn build_path_map(vault_paths: &[String]) -> std::collections::HashMap<String, String> {
    let mut map: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for p in vault_paths {
        // Extract the filename stem: the last component without .md suffix.
        if let Some(filename) = p.rsplit('/').next() {
            if let Some(stem) = filename.strip_suffix(".md") {
                let stem_lower = stem.to_lowercase();
                // Prefer the shortest path for ambiguous names (Obsidian convention).
                let current_len = map.get(&stem_lower).map(|existing| existing.len()).unwrap_or(usize::MAX);
                if p.len() < current_len {
                    map.insert(stem_lower, p.clone());
                }
            }
        }
    }
    map
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
    // index_file is a convenience wrapper that does not provide vault_paths.
    // Backlink tracking is handled by index_directory and watcher which call
    // index_file_with_embedder with vault_paths.
    let empty_map = std::collections::HashMap::new();
    index_file_with_embedder(&IndexParams { db, tokenizer, embedder: None, file_path, vault_name, relative_path, config, path_map: &empty_map })
}

/// Walk `vault_dir`, chunk and index all Markdown files.
/// If `embedder` is Some, also inserts vector embeddings.
/// Returns (per-file results, invalid pattern count).
///
/// # Returns
///
/// `(Vec<(vault_name, relative_path, IndexResult)>, invalid_pattern_count, excluded_file_count)` on success.
/// The caller always destructures the pair; a struct would only add field-name noise.
#[allow(clippy::type_complexity)]
#[tracing::instrument(
    skip(db, tokenizer, config, embedder, progress),
    fields(vault_count = config.vaults.len())
)]
pub fn index_directory(
    db: &NoteDatabase,
    tokenizer: &JapaneseTokenizer,
    config: &IndexConfig,
    embedder: Option<&Embedder>,
    progress: Option<IndexProgress>,
) -> Result<(Vec<(String, String, IndexResult)>, usize, usize), DbError> {
    let mut all_results = Vec::new();
    let mut global_count = 0usize;
    let mut total_invalid = 0usize;
    let mut total_excluded = 0usize;

    for (vault_name, notes_dir) in &config.vaults {
        // Load .shiotsuchiignore patterns and merge with config.exclude_dirs
        let ignore_patterns = load_shiotsuchiignore(notes_dir);
        let mut merged_patterns = config.exclude_dirs.clone();
        merged_patterns.extend(ignore_patterns);

        let (exclude_globset, vault_invalid) = build_exclude_globset(&merged_patterns);
        total_invalid += vault_invalid;
        let mut vault_excluded = 0usize;
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

        // Build the list of indexable file paths for this vault.
        // The same filtering logic applies as in the old per-entry loop.
        let mut vault_paths: Vec<(String, PathBuf)> = Vec::new();
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
                    tracing::warn!("Directory scan error: {}", err);
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
                    tracing::warn!("File path {:?} outside vault root {:?}", path, notes_dir);
                    return false;
                };
                let rel_str = relative.to_string_lossy();
                let is_excluded = exclude_globset.is_match(rel_str.as_ref());
                if is_excluded {
                    vault_excluded += 1;
                    tracing::debug!("Excluded {} (matched exclude pattern)", rel_str);
                }
                !is_excluded
            })
        {
            let path = entry.path();
            let relative = path.strip_prefix(notes_dir).unwrap_or(path);
            let rel_str = relative.to_string_lossy().replace('\\', "/");
            vault_paths.push((rel_str, path.to_path_buf()));
        }

        // Extract just the relative paths for wikilink resolution
        let vault_file_paths: Vec<String> = vault_paths.iter().map(|(rel, _)| rel.clone()).collect();

        // Build a path map for O(1) wikilink resolution (avoids O(N·L) scan per file).
        let path_map = build_path_map(&vault_file_paths);

        for (rel_str, full_path) in &vault_paths {
            global_count += 1;
            if let Some(ref cb) = progress {
                cb(global_count, None);
            }
            let result = index_file_with_embedder(&IndexParams {
                db, tokenizer, embedder, file_path: full_path, vault_name, relative_path: rel_str, config, path_map: &path_map,
            });
            all_results.push((vault_name.clone(), rel_str.to_string(), result));
        }

        // Batch backlink recount: run once per vault after all files are indexed,
        // instead of once per file (which would be O(N²)).
        if config.backlink_scoring && !vault_file_paths.is_empty() {
            if let Err(e) = db.update_backlink_counts_for_vault(vault_name) {
                tracing::warn!("Failed to update backlink counts for vault {}: {}", vault_name, e);
            } else {
                tracing::debug!("Updated backlink counts for vault {}", vault_name);
            }
        }

        total_excluded += vault_excluded;
    }

    Ok((all_results, total_invalid, total_excluded))
}

/// Remove indexed files from DB that no longer exist on disk.
pub fn cleanup_deleted(db: &NoteDatabase, config: &IndexConfig) -> Result<Vec<String>, DbError> {
    let mut removed = Vec::new();
    for (vault_name, notes_dir) in &config.vaults {
        let cached_paths = db.list_cached_paths(vault_name)?;
        let mut vault_removed = false;
        for path in cached_paths {
            let full_path = notes_dir.join(&path);
            if !full_path.exists() {
                // Atomic delete: tag_counts + chunks + file_cache + note_links all in one tx
                if let Err(e) = db.delete_file_fully(vault_name, &path) {
                    tracing::warn!("cleanup_deleted: failed to fully delete {}: {}", path, e);
                }
                removed.push(path);
                vault_removed = true;
            }
        }
        // Recalculate backlink counts if any files were removed
        if vault_removed && config.backlink_scoring {
            if let Err(e) = db.update_backlink_counts_for_vault(vault_name) {
                tracing::warn!("Failed to update backlink counts after cleanup: {}", e);
            }
        }
    }
    Ok(removed)
}

/// Index a single file with optional embedder (for watcher use).
/// `vault_paths` is the list of all relative file paths in the vault, used for
/// resolving wikilinks. Pass an empty slice if backlink scoring is not needed.
/// `path_map` is a pre-built HashMap of lowercase stem → shortest path for O(1)
/// wikilink resolution; build with `build_path_map()`. Pass an empty map if not needed.
pub fn index_file_with_embedder(p: &IndexParams<'_>) -> IndexResult {
    let IndexParams { db, tokenizer, embedder, file_path, vault_name, relative_path, config, path_map } = p;
    let mtime = file_mtime(file_path);
    let file_size = std::fs::metadata(file_path)
        .map(|m| m.len() as i64)
        .unwrap_or(0);

    // Fast path: skip if both mtime and file_size match cached values (avoids reading the file).
    // Uses millisecond-precision mtime + file_size for two-stage check to handle rapid successive writes.
    let model_id = embedder.map_or("none", |e| e.model_id());
    if let Ok(Some(cached_mtime)) = db.cached_mtime(vault_name, relative_path) {
        if let Ok(Some(cached_size)) = db.cached_file_size(vault_name, relative_path) {
            if cached_mtime == mtime && cached_size == file_size {
                return IndexResult::Skipped;
            }
        }
    }

    let ext = file_path.extension().and_then(|e| e.to_str()).unwrap_or("").to_string();

    #[allow(unused_mut)]
    let mut content = {
        if ext == "pdf" {
            if config.enable_pdf_extraction {
                #[cfg(feature = "pdf")]
                {
                    match crate::pdf::extract_text(file_path) {
                        Ok(text) => text,
                        Err(e) => {
                            tracing::warn!("PDF extraction error for {}: {}; falling back to VLM if configured", relative_path, e);
                            String::new()
                        }
                    }
                }
                #[cfg(not(feature = "pdf"))]
                {
                    String::new()
                }
            } else {
                String::new()
            }
        } else {
            match fs::read_to_string(file_path) {
                Ok(c) => c,
                Err(e) => return IndexResult::Error(format!("Read error: {}", e)),
            }
        }
    };

    // If native PDF extraction returned empty text, try VLM for scanned PDFs
    let mut vlm_hash: Option<String> = None;
    if ext == "pdf" && content.is_empty() && config.vlm_enabled && config.vlm_consent_obtained {
        // VLM cache: compute PDF binary hash and compare with cached value
        let pdf_binary_hash = match std::fs::read(file_path) {
            Ok(bytes) => sha256_bytes(&bytes),
            Err(e) => return IndexResult::Error(format!("Read error: {}", e)),
        };
        vlm_hash = Some(pdf_binary_hash.clone());

        let vlm_cache_hit = match db.cached_vlm_hash(vault_name, relative_path) {
            Ok(Some(ref cached)) if *cached == pdf_binary_hash => {
                // VLM cache hit: content unchanged since last VLM extraction.
                // Re-read the previously extracted text from chunks.
                match db.get_chunks_for_file(vault_name, relative_path) {
                    Ok(chunks) if !chunks.is_empty() => {
                        content = chunks.iter().map(|c| c.content.as_str()).collect::<Vec<_>>().join("\n");
                        true
                    }
                    _ => false, // No cached chunks, need to call VLM
                }
            }
            _ => false,
        };

        if !vlm_cache_hit {
            // VLM cache miss: call the API
            use std::sync::atomic::{AtomicBool, Ordering};
            static VLM_WARNING_SENT: AtomicBool = AtomicBool::new(false);
            if !VLM_WARNING_SENT.swap(true, Ordering::Relaxed) {
                tracing::warn!(
                    "VLM extraction enabled: PDF content will be sent to {} API for text extraction. \
                     Set [vlm] enabled = false to disable.",
                    config.vlm_provider
                );
            }

            #[cfg(feature = "vlm")]
            {
                use crate::config::VlmConfig;
                let vlm_config = VlmConfig {
                    enabled: true,
                    consent_obtained: config.vlm_consent_obtained,
                    provider: config.vlm_provider.clone(),
                    endpoint: None,
                    model: config.vlm_model.clone(),
                    max_pages_per_doc: config.vlm_max_pages_per_doc,
                };
                match crate::vlm::extract_text_with_vlm(file_path, &vlm_config) {
                    Ok(Some(text)) => content = text,
                    Ok(None) => {}, // VLM returned nothing, keep empty
                    Err(e) => {
                        tracing::warn!("VLM extraction failed for {}: {}", relative_path, e);
                        // keep empty, fall back to native result
                    }
                }
            }
            #[cfg(not(feature = "vlm"))]
            {
                // VLM feature not compiled, keep empty
            }
        }
    }

    let hash = sha256_hex(&content);

    let is_update = match db.cached_hash(vault_name, relative_path) {
        Ok(Some(cached)) if cached == hash => return IndexResult::Skipped,
        Ok(Some(_)) => true,
        Ok(None) => false,
        Err(e) => return IndexResult::Error(e.to_string()),
    };

    let mut chunks = split_into_chunks(&content, tokenizer, relative_path, vault_name, &config.user_dictionary);

    for chunk in &mut chunks {
        chunk.emphasized_text = extract_emphasized(&chunk.content);
    }

    let embeddings: Vec<Option<Vec<f32>>> = if let Some(emb) = embedder {
        let texts: Vec<&str> = chunks.iter().map(|c| c.content.as_str()).collect();
        let results = emb.embed_batch(&texts);
        results
            .into_iter()
            .enumerate()
            .map(|(i, result)| match result {
                Ok(e) => Some(e),
                Err(e) => {
                    tracing::warn!("Failed to embed chunk {}: {}", i, e);
                    None
                }
            })
            .collect()
    } else {
        vec![None; chunks.len()]
    };

    // Extract tasks and wikilinks BEFORE the atomic reindex transaction so all
    // derived data is committed atomically with the file cache and chunks.
    let task_records: Vec<Task> = if !content.is_empty() {
        let tasks = extract_tasks(&content);
        tasks.iter().map(|(content, checked, line)| Task {
            id: None,
            vault_name: vault_name.to_string(),
            file_path: relative_path.to_string(),
            content: content.clone(),
            checked: *checked,
            line_number: *line,
        }).collect()
    } else {
        Vec::new()
    };

    let note_link_targets: Vec<String> = if !path_map.is_empty() && config.backlink_scoring {
        let link_names = extract_wikilinks(&content);
        if !link_names.is_empty() {
            link_names.iter()
                .filter_map(|name| path_map.get(&name.to_lowercase()).cloned())
                .collect()
        } else {
            Vec::new()
        }
    } else {
        Vec::new()
    };

    if let Err(e) = db.reindex_file(&ReindexParams {
        vault_name,
        relative_path,
        hash: &hash,
        mtime,
        model_id,
        chunks: &chunks,
        embeddings: &embeddings,
        file_size,
        tasks: &task_records,
        note_link_targets: &note_link_targets,
        vlm_hash: vlm_hash.as_deref(),
    }) {
        return IndexResult::Error(e.to_string());
    }

    if is_update {
        IndexResult::Updated
    } else {
        IndexResult::Inserted
    }
}

/// Extract emphasized text from Markdown content.
/// Finds `==highlight==` and `**bold**` patterns and returns the
/// inner text joined by spaces. Empty string if no matches found.
pub fn extract_emphasized(content: &str) -> String {
    let mut results: Vec<String> = Vec::new();
    let bytes = content.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        // Check for ==...==
        if i + 1 < bytes.len() && bytes[i] == b'=' && bytes[i + 1] == b'=' {
            if let Some(end) = content[i + 2..].find("==") {
                let inner = content[i + 2..i + 2 + end].trim();
                if !inner.is_empty() {
                    results.push(inner.to_string());
                }
                i += 2 + end + 2;
                continue;
            }
            i += 2;
            continue;
        }

        // Check for **...** (excluding ***)
        if i + 2 < bytes.len() && bytes[i] == b'*' && bytes[i + 1] == b'*' && bytes[i + 2] != b'*' {
            if let Some(end) = content[i + 2..].find("**") {
                let inner = content[i + 2..i + 2 + end].trim();
                if !inner.is_empty() {
                    results.push(inner.to_string());
                }
                i += 2 + end + 2;
                continue;
            }
            i += 2;
            continue;
        }

        i += 1;
    }

    results.join(" ")
}

/// Extract task checkbox lines from Markdown content.
/// Returns (content_text, is_checked, line_number) for each task.
pub fn extract_tasks(content: &str) -> Vec<(String, bool, usize)> {
    let mut tasks = Vec::new();
    for (line_number, line) in content.lines().enumerate() {
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix("- [ ] ") {
            tasks.push((rest.to_string(), false, line_number + 1));
        } else if let Some(rest) = trimmed.strip_prefix("- [x] ") {
            tasks.push((rest.to_string(), true, line_number + 1));
        } else if let Some(rest) = trimmed.strip_prefix("- [X] ") {
            // GitHub Flavored Markdown treats both [x] and [X] as checked
            tasks.push((rest.to_string(), true, line_number + 1));
        }
    }
    tasks
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
        let (results, _invalid, _excluded) = index_directory(&db, &tokenizer, &config, None, None).unwrap();
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
        let (results, _invalid, _excluded) = index_directory(&db, &tokenizer, &config, None, None).unwrap();
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

        let (results, _invalid, _excluded) = index_directory(&db, &tokenizer, &config, None, None).unwrap();
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

        let (results, _invalid, _excluded) = index_directory(&db, &tokenizer, &config, None, None).unwrap();
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

        let (results, _invalid, _excluded) = index_directory(&db, &tokenizer, &config, None, None).unwrap();
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

        let (results, _invalid, _excluded) = index_directory(&db, &tokenizer, &config, None, None).unwrap();
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
        // `[invalid` is an invalid glob (unclosed bracket) → skipped silently
        assert!(!set.is_match(r"[invalid/foo.md"), "invalid pattern should be skipped");
    }

    #[test]
    fn test_build_exclude_globset_glob_patterns() {
        // With glob characters no longer escaped, patterns like `draft_*`
        // and `*.tmp` act as actual glob wildcards, not literal `*`.

        let patterns = vec!["draft_*".to_string(), "*.tmp".to_string()];
        let (set, _count) = build_exclude_globset(&patterns);

        // `draft_*` is wrapped as `**/draft_*/**` → matches dirs starting with `draft_`
        assert!(set.is_match("draft_2024/foo.md"), "draft_* should match draft_2024 directory");
        assert!(!set.is_match("released_2024/foo.md"), "draft_* should not match released_2024");

        // `*.tmp` is wrapped as `**/*.tmp/**` → matches dirs ending in `.tmp`
        assert!(set.is_match("work.tmp/notes.md"), "*.tmp should match work.tmp directory");
        assert!(!set.is_match("work_tmp/notes.md"), "*.tmp should not match work_tmp");

        // With `draft_*`, non-matching paths pass through
        assert!(!set.is_match("src/main.rs"), "unrelated path should not match");
    }

    #[test]
    fn test_build_exclude_globset_slash_patterns() {
        // Patterns containing '/' are treated as path patterns (not dir-only).
        let patterns = vec!["private/".to_string(), "**/secret/*".to_string()];
        let (set, _count) = build_exclude_globset(&patterns);

        assert!(set.is_match("private/foo.md"), "private/ should match files under private/");
        assert!(set.is_match("a/private/foo.md"), "private/ should match nested too");
        assert!(!set.is_match("public/foo.md"), "public/ should not match");
        assert!(set.is_match("team/secret/plan.md"), "secret/* should match files in secret dir");
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

        let (results, _invalid, _excluded) = index_directory(&db, &tokenizer, &config, None, None).unwrap();
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
        let (results, _invalid, _excluded) = index_directory(&db, &tokenizer, &config, None, None).unwrap();
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
        let (results, _invalid, _excluded) = index_directory(&db, &tokenizer, &config, None, None).unwrap();
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
        let (results, _invalid, _excluded) = index_directory(&db, &tokenizer, &config, None, None).unwrap();
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
        let (results, _invalid, _excluded) = index_directory(&db, &tokenizer, &config, None, None).unwrap();
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
        let (results, _invalid, _excluded) = index_directory(&db, &tokenizer, &config, None, None).unwrap();
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
        let (results, _invalid, _excluded) = index_directory(&db, &tokenizer, &config, None, None).unwrap();
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
        let (results, _invalid, _excluded) = index_directory(&db, &tokenizer, &config, None, None).unwrap();
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
    fn test_index_file_skips_via_mtime_file_size_fast_path() {
        let tokenizer = crate::require_tokenizer!(Default::default());
        let temp = TempDir::new().unwrap();
        let vault = temp.path().join("vault");
        fs::create_dir(&vault).unwrap();
        let path = vault.join("test.md");
        fs::write(&path, "Mtime + size fast path test").unwrap();

        let db = NoteDatabase::open_in_memory().unwrap();
        let config = IndexConfig {
            vaults: vec![("default".to_string(), vault.clone())],
            ..Default::default()
        };

        // First call: insert
        let r1 = index_file(&db, &tokenizer, &path, "default", "test.md", &config);
        assert_eq!(r1, IndexResult::Inserted);

        // Verify both cached_mtime and cached_file_size have values
        assert!(db.cached_mtime("default", "test.md").unwrap().is_some());
        assert!(db.cached_file_size("default", "test.md").unwrap().is_some());

        // Second call with same content → skipped (mtime + size match)
        let r2 = index_file(&db, &tokenizer, &path, "default", "test.md", &config);
        assert_eq!(r2, IndexResult::Skipped);
    }

    #[test]
    fn test_index_file_reindexes_on_file_size_change_only() {
        let tokenizer = crate::require_tokenizer!(Default::default());
        let temp = TempDir::new().unwrap();
        let vault = temp.path().join("vault");
        fs::create_dir(&vault).unwrap();
        let path = vault.join("test.md");
        fs::write(&path, "Initial content for size change test").unwrap();

        let db = NoteDatabase::open_in_memory().unwrap();
        let config = IndexConfig {
            vaults: vec![("default".to_string(), vault.clone())],
            ..Default::default()
        };

        // First call: insert
        let r1 = index_file(&db, &tokenizer, &path, "default", "test.md", &config);
        assert_eq!(r1, IndexResult::Inserted);

        // Overwrite the file with different content (size will change)
        // Force a sleep to ensure mtime actually changes due to filesystem limitations
        std::thread::sleep(std::time::Duration::from_millis(10));
        fs::write(&path, "Different content that changes file size significantly").unwrap();

        // Second call: should detect change (either mtime or size or both)
        let r2 = index_file(&db, &tokenizer, &path, "default", "test.md", &config);
        assert_eq!(r2, IndexResult::Updated);
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
    }

    #[test]
    fn test_escape_glob_literal_basic() {
        assert_eq!(escape_glob_literal("normal"), "normal");
        assert_eq!(escape_glob_literal("path/to/file"), "path/to/file");
    }

    #[test]
    fn test_escape_glob_literal_special_chars() {
        // Glob meta-characters (*, ?, [...]) are NOT escaped anymore;
        // only backslash is escaped.
        assert_eq!(escape_glob_literal("file*"), "file*");
        assert_eq!(escape_glob_literal("file?"), "file?");
        assert_eq!(escape_glob_literal("[test]"), "[test]");
        assert_eq!(escape_glob_literal("{a,b}"), "{a,b}");
        assert_eq!(escape_glob_literal("back\\slash"), "back\\\\slash");
    }

    #[test]
    fn test_escape_glob_literal_multiple_special() {
        // Meta-chars pass through; only backslash is doubled.
        assert_eq!(escape_glob_literal("a*b?c[d]e{f}g"), "a*b?c[d]e{f}g");
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
        assert_eq!(invalid, 1, "unclosed bracket [ should be invalid as glob");
        assert!(!set.is_match("projects/[/notes.md"), "invalid pattern should not match");
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

        let (results, invalid, _excluded) = index_directory(&db, &tokenizer, &config, None, Some(progress)).unwrap();
        assert_eq!(results.len(), 1, "should index 1 file");
        assert!(!results[0].1.is_empty(), "should have a relative path");
        assert_eq!(invalid, 0, "no invalid patterns");
    }

    // ── build_exclude_globset literal pattern matching ───────────────

    #[test]
    fn test_build_exclude_globset_literal_with_brackets() {
        // Brackets are glob meta-characters; `[test]` is a character class.
        let patterns = vec!["[test]".to_string()];
        let (set, invalid) = build_exclude_globset(&patterns);
        assert_eq!(invalid, 0);
        assert!(set.is_match("dir/t/notes.md"), "[test] as char class should match 't'");
        assert!(set.is_match("dir/s/notes.md"), "[test] as char class should match 's'");
        assert!(!set.is_match("dir/z/notes.md"), "[test] should not match 'z'");
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
            "*?[]{},"
        );
    }

    #[test]
    fn test_index_pdf_inserted_without_pdf_feature() {
        // Even with pdf feature OFF (default for this test), .pdf should be Inserted
        // with empty content (not Error from fs::read_to_string on binary data)
        let tokenizer = crate::require_tokenizer!(Default::default());
        let temp = TempDir::new().unwrap();
        let vault = temp.path().join("vault");
        fs::create_dir(&vault).unwrap();
        // Write binary PDF bytes that would cause fs::read_to_string to fail with UTF-8 error
        fs::write(vault.join("report.pdf"), b"%PDF-1.4\x80\x81\xff").unwrap();
        let db = NoteDatabase::open_in_memory().unwrap();
        let config = IndexConfig {
            vaults: vec![("default".to_string(), vault.clone())],
            include_extensions: vec!["pdf".to_string()],
            ..Default::default()
        };
        let (results, _invalid, _excluded) =
            index_directory(&db, &tokenizer, &config, None, None).unwrap();
        assert_eq!(results.len(), 1, "PDF should be indexed");
        assert_eq!(results[0].2, IndexResult::Inserted, "should be Inserted, not Error");
    }

    #[cfg(feature = "pdf")]
    #[test]
    fn test_index_pdf_text_is_searchable_with_pdf_feature() {
        use crate::models::SearchMode;
        use crate::search::{search, SearchRequest};
        use std::collections::HashMap;

        let tokenizer = crate::require_tokenizer!(Default::default());
        let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/hello.pdf");
        if !fixture.exists() {
            eprintln!("SKIP: fixture not found");
            return;
        }

        let temp = TempDir::new().unwrap();
        let vault = temp.path().join("vault");
        fs::create_dir(&vault).unwrap();
        fs::copy(&fixture, vault.join("hello.pdf")).unwrap();

        let db = NoteDatabase::open_in_memory().unwrap();
        let config = IndexConfig {
            vaults: vec![("default".to_string(), vault.clone())],
            include_extensions: vec!["pdf".to_string()],
            ..Default::default()
        };

        let (results, _invalid, _excluded) =
            index_directory(&db, &tokenizer, &config, None, None).unwrap();
        assert_eq!(results.len(), 1, "should index 1 PDF");
        assert_eq!(results[0].2, IndexResult::Inserted);

        let hits = search(
            &db, &tokenizer, &SearchRequest {
                query: "Hello",
                limit: 10,
                mode: SearchMode::Fts,
                embedder: None,
                min_score: None,
                vault_filter: Some("default"),
                tag_filter: None,
                since_date: None,
                user_dictionary: &[],
                synonyms: &HashMap::new(),
                fuzzy: false,
                hybrid_alpha: None,
                mmr: false,
                lambda: 0.5,
                backlink_scoring: false,
                cursor: None,
            },
        ).unwrap().results;
        assert!(
            !hits.is_empty(),
            "should find 'Hello' in indexed PDF, but got no results"
        );
        assert_eq!(hits[0].file_path, "hello.pdf");
    }

    #[test]
    fn test_index_pdf_disabled_extraction_still_inserts() {
        // When enable_pdf_extraction=false, PDF files should still be Inserted
        // (not Error) with empty content.
        let tokenizer = crate::require_tokenizer!(Default::default());
        let temp = TempDir::new().unwrap();
        let vault = temp.path().join("vault");
        fs::create_dir(&vault).unwrap();
        fs::write(vault.join("report.pdf"), b"%PDF-1.4\x80\x81\xff").unwrap();
        let db = NoteDatabase::open_in_memory().unwrap();
        let config = IndexConfig {
            vaults: vec![("default".to_string(), vault.clone())],
            include_extensions: vec!["pdf".to_string()],
            enable_pdf_extraction: false,
            ..Default::default()
        };
        let (results, _invalid, _excluded) =
            index_directory(&db, &tokenizer, &config, None, None).unwrap();
        assert_eq!(results.len(), 1, "PDF should be indexed");
        assert_eq!(results[0].2, IndexResult::Inserted, "should be Inserted, not Error");
    }

    // ── Wikilink extraction ─────────────────────────────────────

    #[test]
    fn test_extract_wikilinks_basic() {
        let content = "Link to [[Note Name]] here";
        let links = extract_wikilinks(content);
        assert_eq!(links, vec!["Note Name"]);
    }

    #[test]
    fn test_extract_wikilinks_with_pipe() {
        let content = "Link to [[Note Name|display text]] here";
        let links = extract_wikilinks(content);
        assert_eq!(links, vec!["Note Name"]);
    }

    #[test]
    fn test_extract_wikilinks_multiple() {
        let content = "[[Note A]] and [[Note B|alias]] and [[Note C]]";
        let links = extract_wikilinks(content);
        assert_eq!(links, vec!["Note A", "Note B", "Note C"]);
    }

    #[test]
    fn test_extract_wikilinks_none() {
        let content = "No wikilinks here";
        let links = extract_wikilinks(content);
        assert!(links.is_empty());
    }

    #[test]
    fn test_extract_wikilinks_empty_content() {
        let links = extract_wikilinks("");
        assert!(links.is_empty());
    }

    #[test]
    fn test_extract_wikilinks_trimmed() {
        let content = "[[  Spaced Name  ]]";
        let links = extract_wikilinks(content);
        assert_eq!(links, vec!["Spaced Name"]);
    }

    #[test]
    fn test_extract_wikilinks_with_unicode() {
        let content = "[[日本語ノート]] and [[プロジェクト計画|Project Plan]]";
        let links = extract_wikilinks(content);
        assert_eq!(links, vec!["日本語ノート", "プロジェクト計画"]);
    }

    // ── build_path_map ──────────────────────────────────────────

    #[test]
    fn test_build_path_map_exact_match() {
        let paths = vec!["note.md".to_string(), "other.md".to_string()];
        let map = build_path_map(&paths);
        assert_eq!(map.get("note"), Some(&"note.md".to_string()));
    }

    #[test]
    fn test_build_path_map_no_match() {
        let paths = vec!["note.md".to_string()];
        let map = build_path_map(&paths);
        assert_eq!(map.get("missing"), None);
    }

    #[test]
    fn test_build_path_map_ambiguous_prefers_shortest() {
        let paths = vec![
            "long/path/note.md".to_string(),
            "note.md".to_string(),
            "other/note.md".to_string(),
        ];
        let map = build_path_map(&paths);
        assert_eq!(map.get("note"), Some(&"note.md".to_string()));
    }

    #[test]
    fn test_build_path_map_subdir_prefers_shorter() {
        let paths = vec![
            "subdir/note.md".to_string(),
            "very/long/path/note.md".to_string(),
        ];
        let map = build_path_map(&paths);
        assert_eq!(map.get("note"), Some(&"subdir/note.md".to_string()));
    }

    #[test]
    fn test_build_path_map_extension_is_stripped_in_link() {
        let paths = vec!["project.md".to_string()];
        let map = build_path_map(&paths);
        assert_eq!(map.get("project"), Some(&"project.md".to_string()));
    }

    #[test]
    fn test_build_path_map_non_md_files_ignored() {
        let paths = vec!["image.png".to_string(), "note.md".to_string()];
        let map = build_path_map(&paths);
        assert_eq!(map.get("note"), Some(&"note.md".to_string()));
        assert_eq!(map.get("image"), None);
    }

    #[test]
    fn test_build_path_map_lowercase_key() {
        let paths = vec!["Note_Name.md".to_string()];
        let map = build_path_map(&paths);
        assert_eq!(map.get("note_name"), Some(&"Note_Name.md".to_string()));
    }

    // ── Backlink indexing integration ───────────────────────────

    #[test]
    fn test_index_directory_updates_backlinks() {
        let tokenizer = crate::require_tokenizer!(Default::default());
        let temp = TempDir::new().unwrap();
        let vault = temp.path().join("vault");
        fs::create_dir(&vault).unwrap();

        // Create a hub note and two notes that link to it
        fs::write(vault.join("hub.md"), "# Hub Note\n\nMain content here.").unwrap();
        fs::write(vault.join("note_a.md"), "# Note A\n\nSee [[Hub Note]] for details.").unwrap();
        fs::write(vault.join("note_b.md"), "# Note B\n\nRelated to [[Hub Note]] and [[Note A]].").unwrap();

        let db = NoteDatabase::open_in_memory().unwrap();
        let config = IndexConfig {
            vaults: vec![("default".to_string(), vault.clone())],
            backlink_scoring: true,
            ..Default::default()
        };

        let (results, _invalid, _excluded) = index_directory(&db, &tokenizer, &config, None, None).unwrap();
        assert_eq!(results.len(), 3);

        // Check that hub.md has backlink_count = 2
        let count: i64 = db.write_conn.borrow().query_row(
            "SELECT backlink_count FROM file_cache WHERE vault_name = 'default' AND path = 'hub.md'",
            [],
            |r| r.get(0),
        ).unwrap();
        assert_eq!(count, 2, "hub.md should have 2 backlinks");

        // Check that note_a.md has backlink_count = 1
        let count_a: i64 = db.write_conn.borrow().query_row(
            "SELECT backlink_count FROM file_cache WHERE vault_name = 'default' AND path = 'note_a.md'",
            [],
            |r| r.get(0),
        ).unwrap();
        assert_eq!(count_a, 1, "note_a.md should have 1 backlink");

        // note_b.md should have 0 backlinks (no one links to it)
        let count_b: i64 = db.write_conn.borrow().query_row(
            "SELECT backlink_count FROM file_cache WHERE vault_name = 'default' AND path = 'note_b.md'",
            [],
            |r| r.get(0),
        ).unwrap();
        assert_eq!(count_b, 0, "note_b.md should have 0 backlinks");
    }

    #[test]
    fn test_index_directory_backlinks_vault_scoped() {
        let tokenizer = crate::require_tokenizer!(Default::default());
        let temp = TempDir::new().unwrap();
        let vault_a = temp.path().join("vault_a");
        let vault_b = temp.path().join("vault_b");
        fs::create_dir_all(&vault_a).unwrap();
        fs::create_dir_all(&vault_b).unwrap();

        // Vault A: hub_a linked by one note
        fs::write(vault_a.join("hub.md"), "# Hub A").unwrap();
        fs::write(vault_a.join("note.md"), "See [[hub]] for info.").unwrap();

        // Vault B: hub_b also linked by one note (same filename, different vault)
        fs::write(vault_b.join("hub.md"), "# Hub B").unwrap();
        fs::write(vault_b.join("other.md"), "See [[hub]] for info.").unwrap();

        let db = NoteDatabase::open_in_memory().unwrap();
        let config = IndexConfig {
            vaults: vec![
                ("vault_a".to_string(), vault_a.clone()),
                ("vault_b".to_string(), vault_b.clone()),
            ],
            backlink_scoring: true,
            ..Default::default()
        };

        let (results, _invalid, _excluded) = index_directory(&db, &tokenizer, &config, None, None).unwrap();
        assert_eq!(results.len(), 4);

        // Each vault's hub should have backlink_count = 1 (scoped to their vault)
        let count_a: i64 = db.write_conn.borrow().query_row(
            "SELECT backlink_count FROM file_cache WHERE vault_name = 'vault_a' AND path = 'hub.md'",
            [],
            |r| r.get(0),
        ).unwrap();
        assert_eq!(count_a, 1, "hub.md in vault_a should have 1 backlink");

        let count_b: i64 = db.write_conn.borrow().query_row(
            "SELECT backlink_count FROM file_cache WHERE vault_name = 'vault_b' AND path = 'hub.md'",
            [],
            |r| r.get(0),
        ).unwrap();
        assert_eq!(count_b, 1, "hub.md in vault_b should have 1 backlink");
    }
}
