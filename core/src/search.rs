use crate::{
    db::{DbError, NoteDatabase},
    models::SearchResult,
    tokenizer::{simple_and_query, JapaneseTokenizer},
};
use std::{fs, path::Path};

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

    // スニペットは元ファイルから抽出（FTS5 の highlight() は使えないため）
    for result in &mut results {
        let file_path = notes_dir.join(&result.path);
        if let Ok(content) = fs::read_to_string(&file_path) {
            result.snippet = extract_snippet(&content, query, 3);
        }
    }

    Ok(results)
}

/// Extract a 3-line snippet around the first query token match.
pub fn extract_snippet(text: &str, query: &str, max_lines: usize) -> String {
    let tokens: Vec<&str> = query.split_whitespace().collect();
    if tokens.is_empty() {
        return text.chars().take(200).collect::<String>() + "…";
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
        None => return text.chars().take(200).collect::<String>() + "…",
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

    if result.len() > 500 {
        result.chars().take(500).collect::<String>() + "…"
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
}
