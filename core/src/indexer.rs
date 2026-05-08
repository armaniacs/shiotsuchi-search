use crate::{
    db::{DbError, NoteDatabase},
    models::{IndexConfig, IndexResult},
    tokenizer::JapaneseTokenizer,
};
use globset::{Glob, GlobSet, GlobSetBuilder};
use pulldown_cmark::{Event, Parser};
use rayon::prelude::*;
use sha2::{Digest, Sha256};
use std::{fs, io, path::Path, time::SystemTime};
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

/// Build a GlobSet from exclude_dirs for gitignore-style component matching.
///
/// Each pattern is wrapped as `**/{pat}/**` so that it matches when `{pat}`
/// appears as any path component (directory name) at any depth. For example,
/// `"node_modules"` matches `node_modules/foo.md` and `a/node_modules/b/c.md`
/// but not `my-node_modules/foo.md`.
///
/// Invalid patterns (e.g., unterminated character classes) are skipped with
/// a warning rather than failing the entire index operation.
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

/// Extract YAML frontmatter from markdown content.
/// Returns (title, body_without_frontmatter).
/// If no frontmatter, returns (None, original_content).
pub fn extract_frontmatter(content: &str) -> (Option<String>, String) {
    if !content.starts_with("---\n") && !content.starts_with("---\r\n") {
        return (None, content.to_string());
    }
    let end_marker = "\n---\n";
    let end_marker_crlf = "\r\n---\r\n";
    if let Some(end_pos) = content.find(end_marker) {
        let frontmatter = &content[4..end_pos];
        let body = &content[end_pos + end_marker.len()..];
        return (parse_yaml_title(frontmatter), body.to_string());
    }
    if let Some(end_pos) = content.find(end_marker_crlf) {
        let frontmatter = &content[4..end_pos];
        let body = &content[end_pos + end_marker_crlf.len()..];
        return (parse_yaml_title(frontmatter), body.to_string());
    }
    (None, content.to_string())
}

fn parse_yaml_title(frontmatter: &str) -> Option<String> {
    for line in frontmatter.lines() {
        if let Some(stripped) = line.trim().strip_prefix("title:") {
            let value = stripped.trim().trim_matches('"').trim_matches('\'');
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }
    None
}

/// Parse Markdown to plain text (strips all markup).
pub fn markdown_to_text(markdown: &str) -> String {
    let parser = Parser::new(markdown);
    let mut text = String::new();
    for event in parser {
        match event {
            Event::Text(t) => text.push_str(&t),
            Event::Code(c) => text.push_str(&c),
            Event::HardBreak | Event::SoftBreak => text.push('\n'),
            _ => {}
        }
    }
    text.lines()
        .map(|l| l.trim())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Derive title from filename stem (hyphens/underscores → spaces).
pub fn title_from_path(path: &Path) -> String {
    path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Untitled")
        .replace(['-', '_'], " ")
}

fn compute_hash(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    hex::encode(hasher.finalize())
}

/// Internal result of parallel file processing (everything before DB upsert).
struct PreparedFile {
    relative_path: String,
    hash: String,
    mtime: i64,
    title: String,
    plain_text: String,
}

/// Read a file, compute its hash, extract metadata, and tokenize the content.
fn prepare_file(
    path: &Path,
    relative_path: &str,
    tokenizer: &JapaneseTokenizer,
) -> Result<PreparedFile, String> {
    let content = fs::read_to_string(path).map_err(|e| format!("Read error: {}", e))?;
    let hash = compute_hash(&content);
    let mtime = fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let (frontmatter_title, body) = extract_frontmatter(&content);
    let title = frontmatter_title.unwrap_or_else(|| title_from_path(path));
    let plain_text = markdown_to_text(&body);
    let tokenized = tokenizer.split(&plain_text);
    Ok(PreparedFile {
        relative_path: relative_path.to_string(),
        hash,
        mtime,
        title,
        plain_text: tokenized,
    })
}

/// Index a single file into the database.
/// `tokenizer` は呼び出し側が一度だけ初期化して渡す（モデルロードコストを1回に抑える）。
pub fn index_file(
    db: &NoteDatabase,
    tokenizer: &JapaneseTokenizer,
    file_path: &Path,
    relative_path: &str,
    _config: &IndexConfig,
) -> IndexResult {
    match prepare_file(file_path, relative_path, tokenizer) {
        Ok(prep) => match db.upsert_note(
            &prep.relative_path,
            &prep.title,
            &prep.plain_text,
            &prep.hash,
            prep.mtime,
        ) {
            Ok(true) => IndexResult::Inserted,
            Ok(false) => IndexResult::Skipped,
            Err(e) => IndexResult::Error(e.to_string()),
        },
        Err(e) => IndexResult::Error(e),
    }
}

/// Process a chunk of entries in parallel and upsert results to the database.
fn process_chunk(
    db: &NoteDatabase,
    tokenizer: &JapaneseTokenizer,
    notes_dir: &Path,
    chunk: &[walkdir::DirEntry],
) -> Vec<(String, IndexResult)> {
    let prepared: Vec<(String, Result<PreparedFile, String>)> = chunk
        .par_iter()
        .map(|entry| {
            let path = entry.path();
            let relative = path.strip_prefix(notes_dir).unwrap_or(path);
            let rel_str = relative.to_string_lossy().to_string();
            let result = prepare_file(path, &rel_str, tokenizer);
            (rel_str, result)
        })
        .collect();

    let mut results = Vec::with_capacity(prepared.len());
    for (rel_str, prep_result) in prepared {
        match prep_result {
            Ok(prep) => {
                let result = match db.upsert_note(
                    &prep.relative_path,
                    &prep.title,
                    &prep.plain_text,
                    &prep.hash,
                    prep.mtime,
                ) {
                    Ok(true) => IndexResult::Inserted,
                    Ok(false) => IndexResult::Skipped,
                    Err(e) => IndexResult::Error(e.to_string()),
                };
                results.push((prep.relative_path, result));
            }
            Err(e) => {
                results.push((rel_str, IndexResult::Error(e)));
            }
        }
    }
    results
}

/// Walk directory and index all matching files.
/// File reading, hashing, and tokenization run in parallel via rayon.
/// DB writes are serial (NoteDatabase uses RefCell which is !Sync).
pub fn index_directory(
    db: &NoteDatabase,
    tokenizer: &JapaneseTokenizer,
    config: &IndexConfig,
) -> Result<(Vec<(String, IndexResult)>, usize), DbError> {
    let notes_dir = &config.notes_dir;

    let (exclude_globset, invalid_patterns) = build_exclude_globset(&config.exclude_dirs);

    let notes_canonical = if config.follow_links {
        Some(notes_dir.canonicalize().map_err(|e| {
            DbError::Io(io::Error::new(
                io::ErrorKind::NotFound,
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
                // Only skip subdirectories whose name starts with '.',
                // not the vault root itself (depth == 0).
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

    const CHUNK_MAX_ENTRIES: usize = 256;
    const CHUNK_MAX_BYTES: u64 = 25_624_064;

    let mut all_results = Vec::new();
    let mut start = 0;
    while start < entries.len() {
        let mut end = start;
        let mut chunk_bytes = 0u64;
        while end < entries.len() && (end - start) < CHUNK_MAX_ENTRIES {
            let file_size = entries[end].path().metadata().map(|m| m.len()).unwrap_or(0);
            if chunk_bytes > 0 && chunk_bytes + file_size > CHUNK_MAX_BYTES {
                break;
            }
            chunk_bytes += file_size;
            end += 1;
        }
        let chunk_results = process_chunk(db, tokenizer, notes_dir, &entries[start..end]);
        all_results.extend(chunk_results);
        start = end;
    }

    Ok((all_results, invalid_patterns))
}

/// Remove notes from DB that no longer exist on disk.
pub fn cleanup_deleted(db: &NoteDatabase, config: &IndexConfig) -> Result<Vec<String>, DbError> {
    let indexed_paths = db.list_paths()?;
    let mut removed = Vec::new();
    for path in indexed_paths {
        let full_path = config.notes_dir.join(&path);
        if !full_path.exists() {
            db.delete_note(&path)?;
            removed.push(path);
        }
    }
    Ok(removed)
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
        let (results, _invalid) = index_directory(&db, &tokenizer, &config).unwrap();
        assert_eq!(results.len(), 10);
        assert_eq!(db.stats().unwrap().total_notes, 10);
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
        let (results, _invalid) = index_directory(&db, &tokenizer, &config).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, "main.md");
        assert_eq!(db.stats().unwrap().total_notes, 1);
    }

    // テスト戦略: モデル未埋め込み環境では SHIOTSUCHI_MODEL_PATH が未設定のため
    // JapaneseTokenizer::new() は Err になる。その場合は simple_tokenize/simple_and_query
    // を直接呼ぶ別パスでテストする。
    // モデルありの場合は JapaneseTokenizer::new(Default::default()).unwrap() を使う。
    //
    // テスト内では以下のパターンを使う:
    //   let tok = JapaneseTokenizer::new(Default::default())
    //       .unwrap_or_else(|_| panic!("モデルが見つかりません: SHIOTSUCHI_MODEL_PATH を設定してください"));
    //
    // CI での実行: SHIOTSUCHI_MODEL_PATH=models/bccwj-suw+unidic_pos+kana.model.zst cargo test

    #[test]
    fn test_no_frontmatter() {
        let content = "# Hello\n\nWorld";
        let (title, body) = extract_frontmatter(content);
        assert!(title.is_none());
        assert_eq!(body, content);
    }

    #[test]
    fn test_with_frontmatter() {
        let content = "---\ntitle: My Note\ntags: [a, b]\n---\n\n# Body\nText";
        let (title, body) = extract_frontmatter(content);
        assert_eq!(title, Some("My Note".to_string()));
        assert!(body.contains("Body"));
        assert!(!body.contains("---"));
    }

    #[test]
    fn test_markdown_to_text() {
        let md = "# Title\n\n**Bold** text and `code`.\n\n- item1\n- item2";
        let text = markdown_to_text(md);
        assert!(text.contains("Bold"));
        assert!(text.contains("code"));
        assert!(!text.contains("#"));
        assert!(!text.contains("**"));
    }

    #[test]
    fn test_title_from_path() {
        assert_eq!(title_from_path(&PathBuf::from("my-note.md")), "my note");
        assert_eq!(
            title_from_path(&PathBuf::from("dir/file_name.md")),
            "file name"
        );
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

        let (results, _invalid) = index_directory(&db, &tokenizer, &config).unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(db.stats().unwrap().total_notes, 2);
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
        index_directory(&db, &tokenizer, &config).unwrap();
        assert_eq!(db.stats().unwrap().total_notes, 1);

        fs::remove_file(vault.join("old.md")).unwrap();
        let removed = cleanup_deleted(&db, &config).unwrap();
        assert_eq!(removed.len(), 1);
        assert_eq!(db.stats().unwrap().total_notes, 0);
    }

    #[test]
    fn test_hidden_dir_auto_excluded() {
        let tokenizer = crate::require_tokenizer!(Default::default());

        let temp = TempDir::new().unwrap();
        let vault = temp.path().join("vault");
        fs::create_dir(&vault).unwrap();

        // Hidden directories should be auto-excluded by filter_entry
        let hidden = vault.join(".obsidian");
        fs::create_dir(&hidden).unwrap();
        fs::write(hidden.join("note.md"), "# Hidden").unwrap();

        // Visible directory with a file
        let notes = vault.join("notes");
        fs::create_dir(&notes).unwrap();
        fs::write(notes.join("visible.md"), "# Visible").unwrap();

        let db = NoteDatabase::open_in_memory().unwrap();
        let config = IndexConfig {
            notes_dir: vault.clone(),
            ..Default::default()
        };
        // Default auto_exclude_hidden=true
        assert!(config.auto_exclude_hidden);

        let (results, _invalid) = index_directory(&db, &tokenizer, &config).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, "notes/visible.md");
    }

    #[test]
    fn test_hidden_dir_included_when_disabled() {
        let tokenizer = crate::require_tokenizer!(Default::default());

        let temp = TempDir::new().unwrap();
        let vault = temp.path().join("vault");
        fs::create_dir(&vault).unwrap();

        // Hidden directory — should NOT be excluded when auto_exclude_hidden=false
        let hidden = vault.join(".hidden_notes");
        fs::create_dir(&hidden).unwrap();
        fs::write(hidden.join("secret.md"), "# Secret").unwrap();

        let db = NoteDatabase::open_in_memory().unwrap();
        let config = IndexConfig {
            notes_dir: vault.clone(),
            auto_exclude_hidden: false,
            ..Default::default()
        };

        let (results, _invalid) = index_directory(&db, &tokenizer, &config).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, ".hidden_notes/secret.md");
    }

    #[test]
    fn test_exclude_dirs_globset_basename_matching() {
        let tokenizer = crate::require_tokenizer!(Default::default());

        let temp = TempDir::new().unwrap();
        let vault = temp.path().join("vault");
        fs::create_dir(&vault).unwrap();

        // Create directories
        let templates = vault.join("templates");
        fs::create_dir(&templates).unwrap();
        fs::write(templates.join("daily.md"), "# Daily").unwrap();

        // This dir name contains "templates" as substring but is NOT the same component
        // With globset matching (unlike old substring matching), this should NOT be excluded
        let templates_extra = vault.join("templates_extra");
        fs::create_dir(&templates_extra).unwrap();
        fs::write(templates_extra.join("extra.md"), "# Extra").unwrap();

        // Normal dir
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

        let (results, _invalid) = index_directory(&db, &tokenizer, &config).unwrap();
        // templates should be excluded, but templates_extra should NOT be (globset component matching)
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
        // Malformed pattern should not panic; after escaping it becomes valid
        // and is included in the set.
        let patterns = vec![r"[invalid".to_string(), "node_modules".to_string()];
        let (set, _count) = build_exclude_globset(&patterns);
        assert!(set.is_match("node_modules/foo.md"));
        assert!(set.is_match("a/node_modules/foo.md"));
        // The escaped pattern matches a literal directory named "[invalid"
        assert!(set.is_match(r"[invalid/foo.md"));
    }

    #[test]
    fn test_build_exclude_globset_escapes_special_chars() {
        let patterns = vec!["draft_*".to_string(), "*.bak".to_string()];
        let (set, _count) = build_exclude_globset(&patterns);
        // Asterisks are treated as literals, not wildcards.
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
        // Verify that exclude pattern "node_modules" also excludes files
        // nested within node_modules subdirectories.
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

        let (results, _invalid) = index_directory(&db, &tokenizer, &config).unwrap();
        assert!(
            results.is_empty(),
            "all files under node_modules should be excluded"
        );
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
        let (results, _invalid) = index_directory(&db, &tokenizer, &config).unwrap();
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
        let (results, _invalid) = index_directory(&db, &tokenizer, &config).unwrap();
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
        let (results, _invalid) = index_directory(&db, &tokenizer, &config).unwrap();
        assert_eq!(results.len(), 257, "all files should be indexed");
        assert_eq!(db.stats().unwrap().total_notes, 257);
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
        let (results, _invalid) = index_directory(&db, &tokenizer, &config).unwrap();
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
        let (results, _invalid) = index_directory(&db, &tokenizer, &config).unwrap();
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
        let (results, _invalid) = index_directory(&db, &tokenizer, &config).unwrap();
        assert_eq!(
            results.len(),
            256,
            "exactly 256 files should all be indexed"
        );
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
        let (results, _invalid) = index_directory(&db, &tokenizer, &config).unwrap();
        assert!(
            results.is_empty(),
            "external symlink should be rejected and result empty"
        );
    }

    #[test]
    fn test_index_file_and_directory_produce_same_result() {
        let tokenizer = crate::require_tokenizer!(Default::default());
        let temp = TempDir::new().unwrap();
        let vault = temp.path().join("vault");
        fs::create_dir(&vault).unwrap();
        fs::write(vault.join("test.md"), "---\ntitle: Same\n---\n\nContent").unwrap();

        let db1 = NoteDatabase::open_in_memory().unwrap();
        let config1 = IndexConfig {
            notes_dir: vault.clone(),
            ..Default::default()
        };
        let result1 = index_file(
            &db1,
            &tokenizer,
            &vault.join("test.md"),
            "test.md",
            &config1,
        );
        assert_eq!(result1, IndexResult::Inserted);

        let db2 = NoteDatabase::open_in_memory().unwrap();
        let config2 = IndexConfig {
            notes_dir: vault.clone(),
            ..Default::default()
        };
        let (results, _invalid) = index_directory(&db2, &tokenizer, &config2).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].1, IndexResult::Inserted);

        let meta1 = db1.get_metadata("test.md").unwrap();
        let meta2 = db2.get_metadata("test.md").unwrap();
        assert_eq!(meta1.title, meta2.title);
        assert_eq!(meta1.hash, meta2.hash);
        assert_eq!(meta1.path, meta2.path);
    }
}
