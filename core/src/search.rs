use crate::{
    db::{DbError, NoteDatabase},
    models::{ChunkSearchResult, SearchMode},
    tokenizer::{simple_and_query, JapaneseTokenizer},
};
use std::collections::HashMap;

/// Main search entry point. Dispatches to FTS, vec, or hybrid (RRF) mode.
/// When `embedder` is None and mode is Hybrid, falls back to Fts.
pub fn search(
    db: &NoteDatabase,
    tokenizer: &JapaneseTokenizer,
    query: &str,
    limit: usize,
    mode: SearchMode,
    embedder: Option<&crate::embedder::Embedder>,
) -> Result<Vec<ChunkSearchResult>, DbError> {
    if query.trim().is_empty() {
        return Ok(vec![]);
    }

    let effective_mode = if embedder.is_none() && matches!(mode, SearchMode::Vec | SearchMode::Hybrid) {
        SearchMode::Fts
    } else {
        mode
    };

    match effective_mode {
        SearchMode::Fts => search_fts(db, tokenizer, query, limit),
        SearchMode::Vec => search_vec(db, embedder.expect("Vec mode requires embedder"), query, limit),
        SearchMode::Hybrid => search_hybrid(db, tokenizer, embedder.expect("Hybrid mode requires embedder"), query, limit),
    }
}

fn search_fts(
    db: &NoteDatabase,
    tokenizer: &JapaneseTokenizer,
    query: &str,
    limit: usize,
) -> Result<Vec<ChunkSearchResult>, DbError> {
    let fts5_query = tokenizer.and_query(query);
    let fts5_query = if fts5_query.is_empty() {
        simple_and_query(query)
    } else {
        fts5_query
    };
    if fts5_query.is_empty() {
        return Ok(vec![]);
    }

    let hits = db.fts_search(&fts5_query, limit)?;
    if hits.is_empty() {
        return Ok(vec![]);
    }

    let ids: Vec<i64> = hits.iter().map(|(id, _)| *id).collect();
    let score_map: HashMap<i64, f64> = hits.into_iter().collect();
    let chunks = db.get_chunks_by_ids(&ids)?;

    let mut results: Vec<ChunkSearchResult> = chunks
        .into_iter()
        .map(|c| {
            let id = c.id.expect("DB chunk missing id");
            let score = *score_map.get(&id).unwrap_or(&0.0);
            ChunkSearchResult {
                chunk_id: id,
                file_path: c.file_path,
                parent_header: c.parent_header,
                content: c.content,
                score,
                search_mode: SearchMode::Fts,
            }
        })
        .collect();

    // FTS5 BM25 rank: lower = more relevant; sort ascending
    results.sort_by(|a, b| a.score.partial_cmp(&b.score).unwrap_or(std::cmp::Ordering::Equal));
    Ok(results)
}

fn search_vec(
    db: &NoteDatabase,
    embedder: &crate::embedder::Embedder,
    query: &str,
    limit: usize,
) -> Result<Vec<ChunkSearchResult>, DbError> {
    let embedding = embedder
        .embed(query)
        .map_err(|e| DbError::Other(e.to_string()))?;

    let hits = db.vec_search(&embedding, limit)?;
    if hits.is_empty() {
        return Ok(vec![]);
    }

    let ids: Vec<i64> = hits.iter().map(|(id, _)| *id).collect();
    let score_map: HashMap<i64, f64> = hits.into_iter().collect();
    let chunks = db.get_chunks_by_ids(&ids)?;

    let mut results: Vec<ChunkSearchResult> = chunks
        .into_iter()
        .map(|c| {
            let id = c.id.expect("DB chunk missing id");
            let score = *score_map.get(&id).unwrap_or(&f64::MAX);
            ChunkSearchResult {
                chunk_id: id,
                file_path: c.file_path,
                parent_header: c.parent_header,
                content: c.content,
                score,
                search_mode: SearchMode::Vec,
            }
        })
        .collect();

    // Vec distance: lower = more relevant; sort ascending
    results.sort_by(|a, b| a.score.partial_cmp(&b.score).unwrap_or(std::cmp::Ordering::Equal));
    Ok(results)
}

/// Hybrid search using Reciprocal Rank Fusion (RRF) to merge FTS + vec results.
/// RRF score = 1/(k + rank_fts) + 1/(k + rank_vec), higher = more relevant.
fn search_hybrid(
    db: &NoteDatabase,
    tokenizer: &JapaneseTokenizer,
    embedder: &crate::embedder::Embedder,
    query: &str,
    limit: usize,
) -> Result<Vec<ChunkSearchResult>, DbError> {
    const K: f64 = 60.0;

    let fts_results = search_fts(db, tokenizer, query, limit * 2)?;
    let vec_results = search_vec(db, embedder, query, limit * 2)?;

    // Build rank maps: chunk_id → 1-based rank
    let fts_ranks: HashMap<i64, usize> = fts_results
        .iter()
        .enumerate()
        .map(|(i, r)| (r.chunk_id, i + 1))
        .collect();
    let vec_ranks: HashMap<i64, usize> = vec_results
        .iter()
        .enumerate()
        .map(|(i, r)| (r.chunk_id, i + 1))
        .collect();

    // Collect all unique chunk ids
    let mut all_ids: Vec<i64> = fts_ranks.keys().chain(vec_ranks.keys()).copied().collect();
    all_ids.sort_unstable();
    all_ids.dedup();

    // Compute RRF score
    let mut rrf_scores: Vec<(i64, f64)> = all_ids
        .into_iter()
        .map(|id| {
            let fts_contrib = fts_ranks.get(&id).map(|&r| 1.0 / (K + r as f64)).unwrap_or(0.0);
            let vec_contrib = vec_ranks.get(&id).map(|&r| 1.0 / (K + r as f64)).unwrap_or(0.0);
            (id, fts_contrib + vec_contrib)
        })
        .collect();

    // Sort by RRF score descending (higher = more relevant)
    rrf_scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    rrf_scores.truncate(limit);

    if rrf_scores.is_empty() {
        return Ok(vec![]);
    }

    let ids: Vec<i64> = rrf_scores.iter().map(|(id, _)| *id).collect();
    let score_map: HashMap<i64, f64> = rrf_scores.into_iter().collect();
    let chunks = db.get_chunks_by_ids(&ids)?;

    // Build a lookup so we can re-order by score
    let mut chunk_map: HashMap<i64, _> = chunks.into_iter().map(|c| (c.id.expect("DB chunk missing id"), c)).collect();
    let mut results: Vec<ChunkSearchResult> = ids
        .iter()
        .filter_map(|id| {
            chunk_map.remove(id).map(|c| {
                let score = *score_map.get(id).unwrap_or(&0.0);
                ChunkSearchResult {
                    chunk_id: *id,
                    file_path: c.file_path,
                    parent_header: c.parent_header,
                    content: c.content,
                    score,
                    search_mode: SearchMode::Hybrid,
                }
            })
        })
        .collect();

    results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    Ok(results)
}

/// Extract a snippet around the first query token match.
pub fn extract_snippet(text: &str, query: &str, max_chars: usize) -> String {
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

    // Walk back to find the start of the line containing pos
    let before = &text[..pos];
    let start = before.rfind('\n').map(|i| i + 1).unwrap_or(0);

    let snippet_text = &text[start..];
    let result: String = snippet_text.lines().take(5).collect::<Vec<_>>().join("\n");

    if result.chars().count() > max_chars {
        result.chars().take(max_chars).collect::<String>() + "…"
    } else {
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::NoteDatabase;
    use crate::models::Chunk;

    #[test]
    fn test_extract_snippet_found() {
        let text = "Line one\nLine two\nLine three\nLine four\nLine five";
        let snippet = extract_snippet(text, "three", 1000);
        assert!(snippet.contains("three"));
    }

    #[test]
    fn test_extract_snippet_fallback_on_no_match() {
        let text = "some content here";
        let snippet = extract_snippet(text, "nonexistent", 200);
        assert!(snippet.ends_with("…"));
    }

    #[test]
    fn test_extract_snippet_empty_query() {
        let text = "Some content without tokens";
        let snippet = extract_snippet(text, "", 200);
        assert!(snippet.ends_with("…"));
    }

    #[test]
    fn test_extract_snippet_respects_max_chars() {
        let text = "A very long line that exceeds the max_chars limit set for snippet extraction";
        let snippet = extract_snippet(text, "exceeds", 20);
        assert!(snippet.ends_with("…"));
        assert!(snippet.chars().count() <= 21);
    }

    #[test]
    fn test_search_fts_finds_chunk() {
        let db = NoteDatabase::open_in_memory().unwrap();
        let tokenizer = crate::require_tokenizer!(crate::tokenizer::TokenizerConfig::default());
        let chunks = vec![Chunk {
            id: None,
            file_path: "test.md".into(),
            chunk_index: 0,
            parent_header: None,
            content: "search engine test content".into(),
            tokenized_content: "search engine test content".into(),
        }];
        db.insert_chunks(&chunks).unwrap();

        let results = search(&db, &tokenizer, "search engine", 10, SearchMode::Fts, None).unwrap();
        assert!(!results.is_empty());
        assert_eq!(results[0].file_path, "test.md");
        assert!(matches!(results[0].search_mode, SearchMode::Fts));
    }

    #[test]
    fn test_search_empty_query_returns_empty() {
        let db = NoteDatabase::open_in_memory().unwrap();
        let tokenizer = crate::require_tokenizer!(crate::tokenizer::TokenizerConfig::default());
        let results = search(&db, &tokenizer, "  ", 10, SearchMode::Fts, None).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn test_search_fts_fallback_when_no_embedder() {
        let db = NoteDatabase::open_in_memory().unwrap();
        let tokenizer = crate::require_tokenizer!(crate::tokenizer::TokenizerConfig::default());
        let chunks = vec![Chunk {
            id: None,
            file_path: "a.md".into(),
            chunk_index: 0,
            parent_header: None,
            content: "hybrid fallback test".into(),
            tokenized_content: "hybrid fallback test".into(),
        }];
        db.insert_chunks(&chunks).unwrap();

        // Hybrid with no embedder → falls back to FTS
        let results = search(&db, &tokenizer, "hybrid fallback", 10, SearchMode::Hybrid, None).unwrap();
        assert!(!results.is_empty());
        assert!(matches!(results[0].search_mode, SearchMode::Fts));
    }
}
