use crate::{
    constants,
    db::{DbError, NoteDatabase},
    models::SearchResult,
    tokenizer::{simple_and_query, JapaneseTokenizer},
};
use std::{fs, io, path::Path};

/// 検索のメインエントリポイント。
/// 1. tokenizer.and_query() で FTS5 AND クエリを構築（vaporetto_and_query() と等価）
/// 2. db.search() で BM25 ランキング
/// 3. 元ファイルから extract_snippet() でスニペットを補完
pub fn search(
    db: &NoteDatabase,
    tokenizer: &JapaneseTokenizer,
    notes_dir: &Path,
    query: &str,
    limit: usize,
) -> Result<Vec<SearchResult>, DbError> {
    // vaporetto_and_query() と等価: "東京" AND "検索" AND "エンジン"
    let fts5_query = tokenizer.and_query(query);
    // Vaporetto でトークンが得られない場合（ASCII のみ等）はフォールバック
    let fts5_query = if fts5_query.is_empty() {
        simple_and_query(query)
    } else {
        fts5_query
    };
    if fts5_query.is_empty() {
        return Ok(vec![]);
    }

    let mut results = db.search(&fts5_query, limit)?;

    // Resolve vault root once, outside the loop — fail the entire search if unresolvable.
    let notes_canonical = notes_dir.canonicalize().map_err(|e| {
        DbError::Io(io::Error::new(
            io::ErrorKind::NotFound,
            format!("cannot canonicalize notes_dir: {}", e),
        ))
    })?;

    // スニペットは元ファイルから抽出（FTS5 の highlight() は使えないため）
    // 各ファイルのエラーは個別に処理（1件の失敗が検索全体を壊さない）
    for result in &mut results {
        let file_path = notes_dir.join(&result.path);

        // vault 内制限: notes_dir 外のファイルを読み出さない
        let file_canonical = match file_path.canonicalize() {
            Ok(c) => c,
            Err(_) => {
                result.snippet = String::from("[path outside vault]");
                continue;
            }
        };
        if !file_canonical.starts_with(&notes_canonical) {
            result.snippet = String::from("[path outside vault]");
            continue;
        }

        if let Ok(content) = fs::read_to_string(&file_path) {
            result.snippet = extract_snippet(&content, query, constants::DEFAULT_SNIPPET_LINES);
        }
    }

    Ok(results)
}

/// Extract a snippet around the first query token match.
/// Uses `constants::DEFAULT_SNIPPET_LINES` lines of context by default.
pub fn extract_snippet(text: &str, query: &str, max_lines: usize) -> String {
    let tokens: Vec<&str> = query.split_whitespace().collect();
    if tokens.is_empty() {
        return text
            .chars()
            .take(constants::FALLBACK_SNIPPET_CHARS)
            .collect::<String>()
            + "…";
    }

    let lower_text = text.to_lowercase();
    let mut best_pos: Option<usize> = None;
    for token in &tokens {
        if let Some(pos) = lower_text.find(&token.to_lowercase()) {
            best_pos = Some(best_pos.map_or(pos, |p| p.min(pos)));
        }
    }

    let pos = match best_pos {
        Some(p) => p,
        None => {
            return text
                .chars()
                .take(constants::FALLBACK_SNIPPET_CHARS)
                .collect::<String>()
                + "…"
        }
    };

    let before = &text[..pos];
    let start = if max_lines == 0 {
        pos
    } else {
        let mut newlines = 0;
        let mut idx = pos;
        for (i, c) in before.char_indices().rev() {
            if c == '\n' {
                newlines += 1;
                if newlines > max_lines {
                    idx = i + 1;
                    break;
                }
            }
            if i == 0 {
                idx = 0;
            }
        }
        idx
    };

    let snippet_text = &text[start..];
    let lines: Vec<&str> = snippet_text.lines().take(max_lines * 2 + 1).collect();
    let result = lines.join("\n");

    if result.len() > constants::MAX_SNIPPET_CHARS {
        result
            .chars()
            .take(constants::MAX_SNIPPET_CHARS)
            .collect::<String>()
            + "…"
    } else {
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_snippet_found() {
        let text = "Line one\nLine two\nLine three\nLine four\nLine five";
        let query = "three";
        let snippet = extract_snippet(text, query, 1);
        assert!(snippet.contains("three"));
    }

    #[test]
    fn test_extract_snippet_multiline() {
        let text = "A\nB\nC\nD\nE\nF\nG";
        let query = "E";
        let snippet = extract_snippet(text, query, 1);
        assert!(snippet.contains("E"));
        // Should include context lines
        assert!(snippet.contains("D") || snippet.contains("F"));
    }

    #[test]
    fn test_search_path_traversal_protection() {
        use crate::db::NoteDatabase;
        use crate::tokenizer::TokenizerConfig;

        let temp = tempfile::TempDir::new().unwrap();
        let db_path = temp.path().join("test.db");
        let db = NoteDatabase::open(&db_path).unwrap();
        let tokenizer = crate::require_tokenizer!(TokenizerConfig::default());
        db.upsert_note("../secret.txt", "Secret", "secret body", "h1", 1)
            .unwrap();
        let results = search(&db, &tokenizer, temp.path(), "secret", 10).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].snippet, "[path outside vault]");
    }

    #[test]
    fn test_search_canonicalize_failure_returns_error() {
        use crate::db::NoteDatabase;
        use crate::tokenizer::TokenizerConfig;

        let temp = tempfile::TempDir::new().unwrap();
        let db = NoteDatabase::open_in_memory().unwrap();
        let tokenizer = crate::require_tokenizer!(TokenizerConfig::default());
        db.upsert_note("note.md", "Note", "some body text", "h", 1)
            .unwrap();

        // Use non-existent path to trigger canonicalize failure
        let nonexistent = temp.path().join("nonexistent");
        let result = search(&db, &tokenizer, &nonexistent, "some", 10);
        assert!(
            result.is_err(),
            "search should error when notes_dir cannot be canonicalized"
        );
    }

    #[test]
    fn test_extract_snippet_fallback_chars_on_no_match() {
        let text = "line one\nline two\nline three\nline four\nline five";
        let query = "nonexistent";
        let snippet = extract_snippet(text, query, 1);
        assert!(snippet.ends_with("…"));
        assert!(snippet.len() <= constants::FALLBACK_SNIPPET_CHARS + 1);
    }

    #[test]
    fn test_extract_snippet_empty_query_uses_fallback() {
        let text = "Some content without tokens";
        let snippet = extract_snippet(text, "", 3);
        assert!(snippet.ends_with("…"));
        // Fallback truncates at FALLBACK_SNIPPET_CHARS, but returns early
        // if text is shorter than that limit
        assert!(snippet.len() <= constants::FALLBACK_SNIPPET_CHARS + 1);
    }

    #[test]
    fn test_extract_snippet_zero_context_lines() {
        let text = "before\nmatched\nafter";
        let snippet = extract_snippet(text, "matched", 0);
        assert_eq!(snippet, "matched");
    }

    #[test]
    fn test_search_path_traversal_dotdot_rejected() {
        use crate::db::NoteDatabase;
        use crate::tokenizer::TokenizerConfig;

        let temp = tempfile::TempDir::new().unwrap();
        let db = NoteDatabase::open_in_memory().unwrap();
        let tokenizer = crate::require_tokenizer!(TokenizerConfig::default());
        db.upsert_note("../../etc/passwd", "Evil", "malicious body", "h1", 1)
            .unwrap();
        let results = search(&db, &tokenizer, temp.path(), "malicious", 10).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].snippet, "[path outside vault]");
    }

    #[test]
    fn test_extract_snippet_single_line() {
        let text = "This is a single line of text with a search token in it";
        let snippet = extract_snippet(text, "search", 3);
        assert!(snippet.contains("search"));
        assert_eq!(snippet.lines().count(), 1);
    }
}
