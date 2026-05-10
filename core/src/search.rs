use crate::{
    constants,
    db::{DbError, NoteDatabase},
    models::{SearchConfig, SearchResult},
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
    search_cfg: Option<&SearchConfig>,
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
            let max_chars = match search_cfg {
                Some(cfg) => cfg.max_snippet_chars,
                None => SearchConfig::default().max_snippet_chars,
            };
            result.snippet =
                extract_snippet(&content, query, constants::DEFAULT_SNIPPET_LINES, max_chars);
        }
    }

    Ok(results)
}

/// Extract a snippet around the first query token match.
pub fn extract_snippet(text: &str, query: &str, max_lines: usize, max_chars: usize) -> String {
    let tokens: Vec<&str> = query.split_whitespace().collect();
    if tokens.is_empty() {
        return text.chars().take(max_chars).collect::<String>() + "…";
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
        None => return text.chars().take(max_chars).collect::<String>() + "…",
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

    if result.chars().count() > max_chars {
        result.chars().take(max_chars).collect::<String>() + "…"
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
        let snippet = extract_snippet(text, query, 1, 1000);
        assert!(snippet.contains("three"));
    }

    #[test]
    fn test_extract_snippet_multiline() {
        let text = "A\nB\nC\nD\nE\nF\nG";
        let query = "E";
        let snippet = extract_snippet(text, query, 1, 1000);
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
        let results = search(&db, &tokenizer, temp.path(), "secret", 10, None).unwrap();
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
        let result = search(&db, &tokenizer, &nonexistent, "some", 10, None);
        assert!(
            result.is_err(),
            "search should error when notes_dir cannot be canonicalized"
        );
    }

    #[test]
    fn test_extract_snippet_fallback_chars_on_no_match() {
        let text = "line one\nline two\nline three\nline four\nline five";
        let query = "nonexistent";
        let snippet = extract_snippet(text, query, 1, 200);
        assert!(snippet.ends_with("…"));
        assert!(snippet.chars().count() <= 200 + 1);
    }

    #[test]
    fn test_extract_snippet_empty_query_uses_fallback() {
        let text = "Some content without tokens";
        let snippet = extract_snippet(text, "", 3, 200);
        assert!(snippet.ends_with("…"));
        // Fallback truncates at max_chars, but returns early
        // if text is shorter than that limit
        assert!(snippet.chars().count() <= 200 + 1);
    }

    #[test]
    fn test_extract_snippet_zero_context_lines() {
        let text = "before\nmatched\nafter";
        let snippet = extract_snippet(text, "matched", 0, 1000);
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
        let results = search(&db, &tokenizer, temp.path(), "malicious", 10, None).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].snippet, "[path outside vault]");
    }

    #[test]
    fn test_extract_snippet_single_line() {
        let text = "This is a single line of text with a search token in it";
        let snippet = extract_snippet(text, "search", 3, 1000);
        assert!(snippet.contains("search"));
        assert_eq!(snippet.lines().count(), 1);
    }

    #[test]
    fn test_search_config_clamping() {
        let cfg_low = SearchConfig::new(10);
        assert_eq!(cfg_low.max_snippet_chars, 128);
        let cfg_high = SearchConfig::new(100_000);
        assert_eq!(cfg_high.max_snippet_chars, 65535);
        let cfg_ok = SearchConfig::new(5000);
        assert_eq!(cfg_ok.max_snippet_chars, 5000);
    }

    #[test]
    fn test_extract_snippet_respects_max_chars() {
        let text = "A very long single line that exceeds the max_chars limit set for the snippet extraction function";
        let query = "exceeds";
        // max_chars=40 ensures "exceeds" (pos ~27) is within the limit
        let snippet = extract_snippet(text, query, 3, 40);
        assert!(snippet.contains("exceeds"));
        // When the matched line exceeds max_chars, it gets truncated
        if snippet.ends_with("…") {
            assert!(snippet.chars().count() <= 40 + 1);
        }
    }

    #[test]
    fn test_extract_snippet_truncate_on_long_multiline() {
        let text = "line1\nline2\nline3\nline4 with keyword\nline5\nline6\nline7\nline8\nline9";
        let query = "keyword";
        // max_chars=65: enough to include "keyword" but may still truncate footer
        let snippet = extract_snippet(text, query, 3, 65);
        assert!(snippet.contains("keyword"));
        assert!(snippet.contains("line4"));
        if snippet.ends_with("…") {
            assert!(snippet.chars().count() <= 65 + 1);
        }
    }

    #[test]
    fn test_extract_snippet_match_after_third_line() {
        // Reproduce issue #1: match appears on the 4th line,
        // but max_lines=3 means the snippet includes up to 7 lines total
        let text = "one\ntwo\nthree\nfour with keyword\nfive\nsix\nseven\neight\nnine";
        let query = "keyword";
        let snippet = extract_snippet(text, query, 3, 1000);
        assert!(snippet.contains("keyword"));
        assert!(snippet.contains("four"));
        // With 3 context lines each side, it should show one..seven (7 lines)
        assert_eq!(snippet.lines().count(), 7);
    }

    #[test]
    fn test_extract_snippet_very_short_max_chars_truncates_before_match() {
        // If max_chars is shorter than the match position, the match will be
        // truncated out — this is expected behavior.
        let text = "prefix words here then keyword in the rest";
        let query = "keyword";
        let snippet = extract_snippet(text, query, 3, 15);
        // match is at position ~19, so it's beyond the 15-char truncation
        assert!(!snippet.contains("keyword"));
        assert!(snippet.ends_with("…"));
        assert!(snippet.chars().count() <= 15 + 1);
    }

    #[test]
    fn test_search_with_search_config() {
        use crate::db::NoteDatabase;
        use crate::tokenizer::TokenizerConfig;
        use std::io::Write;

        let temp = tempfile::TempDir::new().unwrap();
        let vault = temp.path().join("vault");
        std::fs::create_dir(&vault).unwrap();

        let mut f = std::fs::File::create(vault.join("note.md")).unwrap();
        let content = format!(
            "Header\n{line}\n{line}\n{line}\nkeyword line\n{line}\n{line}\n{line}\nFooter",
            line = "a".repeat(100)
        );
        writeln!(f, "{}", content).unwrap();
        drop(f);

        let db = NoteDatabase::open_in_memory().unwrap();
        let tokenizer = crate::require_tokenizer!(TokenizerConfig::default());

        let _fts5_query = tokenizer.and_query("keyword"); // validate tokenizer works
        db.upsert_note("note.md", "Title", &content, "h", 1)
            .unwrap();

        // Without SearchConfig: default max_chars = 1000
        let results = search(&db, &tokenizer, &vault, "keyword", 10, None).unwrap();
        assert_eq!(results.len(), 1);
        let default_snippet = &results[0].snippet;

        // With SearchConfig: max_chars = 128
        let small_cfg = SearchConfig::new(128);
        let results2 = search(&db, &tokenizer, &vault, "keyword", 10, Some(&small_cfg)).unwrap();
        assert_eq!(results2.len(), 1);
        let small_snippet = &results2[0].snippet;

        assert!(default_snippet.contains("keyword"));
        assert!(
            default_snippet.chars().count() > small_snippet.chars().count(),
            "default snippet ({}) should be longer than clamped snippet ({})",
            default_snippet.chars().count(),
            small_snippet.chars().count()
        );
        if small_snippet.ends_with("…") {
            assert!(small_snippet.chars().count() <= 128 + 1);
        }
    }
}
