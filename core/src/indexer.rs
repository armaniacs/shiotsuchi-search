use crate::{
    db::{DbError, NoteDatabase},
    models::{IndexConfig, IndexResult},
    tokenizer::JapaneseTokenizer,
};
use pulldown_cmark::{Event, Parser};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::Path,
    time::SystemTime,
};
use walkdir::WalkDir;

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
    text.lines().map(|l| l.trim()).collect::<Vec<_>>().join("\n")
}

/// Derive title from filename stem (hyphens/underscores → spaces).
pub fn title_from_path(path: &Path) -> String {
    path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Untitled")
        .replace('-', " ")
        .replace('_', " ")
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
        .map(|t| t.duration_since(SystemTime::UNIX_EPOCH).unwrap_or_default().as_secs() as i64)
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

/// Walk directory and index all matching files.
/// `tokenizer` を受け取り各ファイルの index_file に渡す。
pub fn index_directory(
    db: &NoteDatabase,
    tokenizer: &JapaneseTokenizer,
    config: &IndexConfig,
) -> Result<Vec<(String, IndexResult)>, DbError> {
    let mut results = Vec::new();
    let notes_dir = &config.notes_dir;

    for entry in WalkDir::new(notes_dir)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if !config.include_extensions.iter().any(|e| e == ext) {
            continue;
        }
        let relative = path.strip_prefix(notes_dir).unwrap_or(path);
        let rel_str = relative.to_string_lossy();
        if config.exclude_patterns.iter().any(|pat| rel_str.contains(pat)) {
            continue;
        }
        // tokenizer を渡して index_file を呼ぶ
        let result = index_file(db, tokenizer, path, &rel_str, config);
        results.push((rel_str.to_string(), result));
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
    use crate::{db::NoteDatabase, tokenizer::JapaneseTokenizer};
    use std::{io::Write, path::PathBuf};
    use tempfile::TempDir;

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
        assert_eq!(title_from_path(&PathBuf::from("dir/file_name.md")), "file name");
    }

    #[test]
    fn test_index_directory() {
        // Skip if tokenizer cannot be created
        let tokenizer = match JapaneseTokenizer::new(Default::default()) {
            Ok(tok) => tok,
            Err(_) => return, // Skip test if model not available
        };

        let temp = TempDir::new().unwrap();
        let vault = temp.path().join("vault");
        fs::create_dir(&vault).unwrap();

        let mut f1 = fs::File::create(vault.join("note1.md")).unwrap();
        writeln!(f1, "# Hello\n\nWorld content").unwrap();
        let mut f2 = fs::File::create(vault.join("note2.md")).unwrap();
        writeln!(f2, "---\ntitle: Special\n---\n\nUnique text here").unwrap();

        let db = NoteDatabase::open_in_memory().unwrap();
        let config = IndexConfig { notes_dir: vault.clone(), ..Default::default() };

        let results = index_directory(&db, &tokenizer, &config).unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(db.stats().unwrap().total_notes, 2);
    }

    #[test]
    fn test_cleanup_deleted() {
        // Skip if tokenizer cannot be created
        let tokenizer = match JapaneseTokenizer::new(Default::default()) {
            Ok(tok) => tok,
            Err(_) => return, // Skip test if model not available
        };

        let temp = TempDir::new().unwrap();
        let vault = temp.path().join("vault");
        fs::create_dir(&vault).unwrap();

        let mut f = fs::File::create(vault.join("old.md")).unwrap();
        writeln!(f, "content").unwrap();

        let db = NoteDatabase::open_in_memory().unwrap();
        let config = IndexConfig { notes_dir: vault.clone(), ..Default::default() };
        index_directory(&db, &tokenizer, &config).unwrap();
        assert_eq!(db.stats().unwrap().total_notes, 1);

        fs::remove_file(vault.join("old.md")).unwrap();
        let removed = cleanup_deleted(&db, &config).unwrap();
        assert_eq!(removed.len(), 1);
        assert_eq!(db.stats().unwrap().total_notes, 0);
    }
}
