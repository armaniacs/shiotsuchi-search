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

/// Build a GlobSet from exclude_patterns for gitignore-style matching.
/// Invalid patterns are skipped with a warning.
fn build_exclude_globset(patterns: &[String]) -> GlobSet {
    let mut builder = GlobSetBuilder::new();
    for pat in patterns {
        // Add two patterns for gitignore-style component matching:
        //   "**/{pat}"    — matches when the pattern is the last path component
        //   "**/{pat}/**" — matches when the pattern is an intermediate component
        for wrapped in [format!("**/{}", pat), format!("**/{}/**", pat)] {
            match Glob::new(&wrapped) {
                Ok(glob) => {
                    builder.add(glob);
                }
                Err(e) => {
                    log::warn!("Invalid exclude pattern {:?} (wrapped as {:?}): {}", pat, wrapped, e);
                }
            }
        }
    }
    builder.build().unwrap_or_else(|e| {
        log::warn!("Failed to build exclude GlobSet: {}", e);
        GlobSet::empty()
    })
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

/// Index a single file into the database.
/// `tokenizer` は呼び出し側が一度だけ初期化して渡す（モデルロードコストを1回に抑える）。
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
    let hash = compute_hash(&content);
    let mtime = fs::metadata(file_path)
        .and_then(|m| m.modified())
        .map(|t| {
            t.duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64
        })
        .unwrap_or(0);

    let (frontmatter_title, body) = extract_frontmatter(&content);
    let title = frontmatter_title.unwrap_or_else(|| title_from_path(file_path));
    let plain_text = markdown_to_text(&body);

    // vaporetto_split(plain_text, ' ') と等価: トークン列を空白区切りで body カラムに格納
    let tokenized = tokenizer.split(&plain_text);

    match db.upsert_note(relative_path, &title, &tokenized, &hash, mtime) {
        Ok(true) => IndexResult::Inserted,
        Ok(false) => IndexResult::Skipped,
        Err(e) => IndexResult::Error(e.to_string()),
    }
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

/// Walk directory and index all matching files.
/// File reading, hashing, and tokenization run in parallel via rayon.
/// DB writes are serial (NoteDatabase uses RefCell which is !Sync).
pub fn index_directory(
    db: &NoteDatabase,
    tokenizer: &JapaneseTokenizer,
    config: &IndexConfig,
) -> Result<Vec<(String, IndexResult)>, DbError> {
    let notes_dir = &config.notes_dir;

    // Pre-compile exclude patterns into a GlobSet for gitignore-style matching.
    let exclude_globset = build_exclude_globset(&config.exclude_patterns);

    // If following links, canonicalize the notes_dir for vault boundary checks.
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

    // Collect matching file entries (serial — WalkDir is not Send)
    let entries: Vec<_> = WalkDir::new(notes_dir)
        .follow_links(config.follow_links)
        .into_iter()
        .filter_entry(|e| {
            if e.file_type().is_dir() {
                // Auto-skip hidden directories when enabled.
                if config.auto_exclude_hidden
                    && e.file_name().to_string_lossy().starts_with('.')
                {
                    return false;
                }
                // Vault boundary check: when following links, ensure the resolved
                // path stays within the vault root (defeats symlink escape).
                if let Some(ref canonical_root) = notes_canonical {
                    match e.path().canonicalize() {
                        Ok(canonical) => {
                            if !canonical.starts_with(canonical_root) {
                                return false;
                            }
                        }
                        Err(_) => return false, // broken symlink or unresolvable
                    }
                }
            }
            true
        })
        .filter_map(|e| e.ok())
        .filter(|entry| {
            let path = entry.path();
            if !path.is_file() {
                return false;
            }
            // Vault boundary check for file entries (defeats symlink-to-file escapes).
            if let Some(ref canonical_root) = notes_canonical {
                match path.canonicalize() {
                    Ok(canonical) => {
                        if !canonical.starts_with(canonical_root) {
                            return false;
                        }
                    }
                    Err(_) => return false, // broken symlink or unresolvable
                }
            }
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            if !config.include_extensions.iter().any(|e| e == ext) {
                return false;
            }
            let relative = path.strip_prefix(notes_dir).unwrap_or(path);
            let rel_str = relative.to_string_lossy();
            // Gitignore-style glob matching via GlobSet.
            // Patterns like "node_modules" match any path component named "node_modules".
            !exclude_globset.is_match(rel_str.as_ref())
        })
        .collect();

    // Parallel: read file, hash, extract frontmatter, tokenize
    let prepared: Vec<(String, Result<PreparedFile, String>)> = entries
        .par_iter()
        .map(|entry| {
            let path = entry.path();
            let relative = path.strip_prefix(notes_dir).unwrap_or(path);
            let rel_str = relative.to_string_lossy().to_string();

            let content = match fs::read_to_string(path) {
                Ok(c) => c,
                Err(e) => return (rel_str, Err(format!("Read error: {}", e))),
            };
            let hash = compute_hash(&content);
            let mtime = fs::metadata(path)
                .and_then(|m| m.modified())
                .map(|t| {
                    t.duration_since(SystemTime::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs() as i64
                })
                .unwrap_or(0);

            let (frontmatter_title, body) = extract_frontmatter(&content);
            let title = frontmatter_title.unwrap_or_else(|| title_from_path(path));
            let plain_text = markdown_to_text(&body);
            let tokenized = tokenizer.split(&plain_text);

            (
                rel_str.clone(),
                Ok(PreparedFile {
                    relative_path: rel_str,
                    hash,
                    mtime,
                    title,
                    plain_text: tokenized,
                }),
            )
        })
        .collect();

    // Serial: DB upserts (RefCell<Connection> is !Sync)
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

    Ok(results)
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
        let results = index_directory(&db, &tokenizer, &config).unwrap();
        assert_eq!(results.len(), 10);
        assert_eq!(db.stats().unwrap().total_notes, 10);
    }

    #[test]
    fn test_index_directory_respects_exclude_patterns() {
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
            exclude_patterns: vec!["templates".to_string()],
            ..Default::default()
        };
        let results = index_directory(&db, &tokenizer, &config).unwrap();
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

        let results = index_directory(&db, &tokenizer, &config).unwrap();
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

        let results = index_directory(&db, &tokenizer, &config).unwrap();
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

        let results = index_directory(&db, &tokenizer, &config).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, ".hidden_notes/secret.md");
    }

    #[test]
    fn test_exclude_patterns_globset_basename_matching() {
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
            exclude_patterns: vec!["templates".to_string()],
            auto_exclude_hidden: false,
            ..Default::default()
        };

        let results = index_directory(&db, &tokenizer, &config).unwrap();
        // templates should be excluded, but templates_extra should NOT be (globset component matching)
        assert_eq!(results.len(), 2, "templates_extra and notes should be indexed, templates excluded");
        let paths: Vec<&str> = results.iter().map(|r| r.0.as_str()).collect();
        assert!(paths.contains(&"notes/main.md"));
        assert!(paths.contains(&"templates_extra/extra.md"));
        assert!(!paths.contains(&"templates/daily.md"));
    }

    #[test]
    fn test_build_exclude_globset_invalid_pattern() {
        // Malformed pattern should not panic; should produce an empty or partial set.
        let patterns = vec![r"[invalid".to_string(), "node_modules".to_string()];
        let set = build_exclude_globset(&patterns);
        // At least the valid "node_modules" patterns should be present.
        assert!(set.is_match("node_modules/foo.md"));
        assert!(set.is_match("a/node_modules/foo.md"));
        // The invalid pattern should not cause a crash
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
            exclude_patterns: vec!["node_modules".to_string()],
            auto_exclude_hidden: false,
            ..Default::default()
        };

        let results = index_directory(&db, &tokenizer, &config).unwrap();
        assert!(results.is_empty(), "all files under node_modules should be excluded");
    }
}
