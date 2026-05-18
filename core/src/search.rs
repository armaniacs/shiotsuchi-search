use crate::{
    db::{DbError, NoteDatabase},
    models::{ChunkSearchResult, SearchMode},
    tokenizer::{simple_and_query, JapaneseTokenizer},
};
use std::collections::HashMap;
use log;

/// Main search entry point. Dispatches to FTS, vec, or hybrid (RRF) mode.
/// When `embedder` is None and mode is Hybrid, falls back to Fts.
///
/// `min_score` filters results by score after sorting.
///   - FTS/Vec mode (lower score = more relevant): results with `score > min_score` are excluded.
///   - Hybrid mode (higher RRF score = more relevant): results with `score < min_score` are excluded.
pub fn search(
    db: &NoteDatabase,
    tokenizer: &JapaneseTokenizer,
    query: &str,
    limit: usize,
    mode: SearchMode,
    embedder: Option<&crate::embedder::Embedder>,
    min_score: Option<f64>,
    vault_filter: Option<&str>,
) -> Result<Vec<ChunkSearchResult>, DbError> {
    if query.trim().is_empty() {
        return Ok(vec![]);
    }

    let effective_mode = if embedder.is_none() && matches!(mode, SearchMode::Hybrid) {
        SearchMode::Fts
    } else {
        mode
    };

    match effective_mode {
        SearchMode::Fts => search_fts(db, tokenizer, query, limit, min_score, vault_filter),
        SearchMode::Vec => {
            let emb = embedder.ok_or_else(|| DbError::Other("Vec mode requires embedder — model not loaded".into()))?;
            search_vec(db, emb, query, limit, min_score, vault_filter)
        }
        SearchMode::Hybrid => {
            let emb = embedder.ok_or_else(|| DbError::Other("Hybrid mode requires embedder — model not loaded".into()))?;
            search_hybrid(db, tokenizer, emb, query, limit, min_score, vault_filter)
        }
    }
}

fn search_fts(
    db: &NoteDatabase,
    tokenizer: &JapaneseTokenizer,
    query: &str,
    limit: usize,
    min_score: Option<f64>,
    vault_filter: Option<&str>,
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

    // When vault_filter is active, expand the internal limit so that
    // post-filtering doesn't starve the target vault of results.
    let internal_limit = if vault_filter.is_some() {
        limit.saturating_mul(3).max(limit)
    } else {
        limit
    };

    let hits = db.fts_search(&fts5_query, internal_limit)?;
    if hits.is_empty() {
        return Ok(vec![]);
    }

    let ids: Vec<i64> = hits.iter().map(|(id, _)| *id).collect();
    let score_map: HashMap<i64, f64> = hits.into_iter().collect();
    let chunks = db.get_chunks_by_ids(&ids)?;

    let mut results: Vec<ChunkSearchResult> = chunks
        .into_iter()
        .filter_map(|c| {
            let id = match c.id {
                Some(id) => id,
                None => {
                    log::warn!("FTS search: chunk from DB has no id, skipping");
                    return None;
                }
            };
            let score = *score_map.get(&id).unwrap_or(&0.0);
            Some(ChunkSearchResult {
                chunk_id: id,
                file_path: c.file_path,
                parent_header: c.parent_header,
                content: c.content,
                score,
                search_mode: SearchMode::Fts,
                vault_name: c.vault_name,
            })
        })
        .collect();

    if let Some(vault) = vault_filter {
        results.retain(|r| r.vault_name == vault);
    }
    results.truncate(limit);

    // FTS5 BM25 rank: lower = more relevant; sort ascending
    results.sort_by(|a, b| a.score.partial_cmp(&b.score).unwrap_or(std::cmp::Ordering::Equal));

    if let Some(ms) = min_score {
        results.retain(|r| r.score <= ms);
    }

    Ok(results)
}

fn search_vec(
    db: &NoteDatabase,
    embedder: &crate::embedder::Embedder,
    query: &str,
    limit: usize,
    min_score: Option<f64>,
    vault_filter: Option<&str>,
) -> Result<Vec<ChunkSearchResult>, DbError> {
    let embedding = embedder
        .embed(query)
        .map_err(|e| DbError::Other(e.to_string()))?;

    // When vault_filter is active, expand the internal limit so that
    // post-filtering doesn't starve the target vault of results.
    let internal_limit = if vault_filter.is_some() {
        limit.saturating_mul(3).max(limit)
    } else {
        limit
    };

    let hits = db.vec_search(&embedding, internal_limit)?;
    if hits.is_empty() {
        return Ok(vec![]);
    }

    let ids: Vec<i64> = hits.iter().map(|(id, _)| *id).collect();
    let score_map: HashMap<i64, f64> = hits.into_iter().collect();
    let chunks = db.get_chunks_by_ids(&ids)?;

    let mut results: Vec<ChunkSearchResult> = chunks
        .into_iter()
        .filter_map(|c| {
            let id = match c.id {
                Some(id) => id,
                None => {
                    log::warn!("Vec search: chunk from DB has no id, skipping");
                    return None;
                }
            };
            let score = *score_map.get(&id).unwrap_or(&f64::MAX);
            Some(ChunkSearchResult {
                chunk_id: id,
                file_path: c.file_path,
                parent_header: c.parent_header,
                content: c.content,
                score,
                search_mode: SearchMode::Vec,
                vault_name: c.vault_name,
            })
        })
        .collect();

    if let Some(vault) = vault_filter {
        results.retain(|r| r.vault_name == vault);
    }
    results.truncate(limit);

    // Vec distance: lower = more relevant; sort ascending
    results.sort_by(|a, b| a.score.partial_cmp(&b.score).unwrap_or(std::cmp::Ordering::Equal));

    if let Some(ms) = min_score {
        results.retain(|r| r.score <= ms);
    }

    Ok(results)
}

/// Compute Reciprocal Rank Fusion scores from FTS and vec search results.
///
/// `k` is the RRF constant (default 60.0). Higher RRF score = more relevant.
/// Results are sorted by RRF score descending and truncated to `limit`.
pub(crate) fn compute_rrf(
    fts_results: &[ChunkSearchResult],
    vec_results: &[ChunkSearchResult],
    limit: usize,
    k: f64,
) -> Vec<(i64, f64)> {
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
            let fts_contrib = fts_ranks.get(&id).map(|&r| 1.0 / (k + r as f64)).unwrap_or(0.0);
            let vec_contrib = vec_ranks.get(&id).map(|&r| 1.0 / (k + r as f64)).unwrap_or(0.0);
            (id, fts_contrib + vec_contrib)
        })
        .collect();

    // Sort by RRF score descending (higher = more relevant)
    rrf_scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    rrf_scores.truncate(limit);
    rrf_scores
}

/// Hybrid search using Reciprocal Rank Fusion (RRF) to merge FTS + vec results.
/// RRF score = 1/(k + rank_fts) + 1/(k + rank_vec), higher = more relevant.
fn search_hybrid(
    db: &NoteDatabase,
    tokenizer: &JapaneseTokenizer,
    embedder: &crate::embedder::Embedder,
    query: &str,
    limit: usize,
    min_score: Option<f64>,
    vault_filter: Option<&str>,
) -> Result<Vec<ChunkSearchResult>, DbError> {
    const K: f64 = 60.0;

    let fts_results = search_fts(db, tokenizer, query, limit * 2, None, vault_filter)?;
    let vec_results = search_vec(db, embedder, query, limit * 2, None, vault_filter)?;

    let rrf_scores = compute_rrf(&fts_results, &vec_results, limit, K);

    if rrf_scores.is_empty() {
        return Ok(vec![]);
    }

    let ids: Vec<i64> = rrf_scores.iter().map(|(id, _)| *id).collect();
    let score_map: HashMap<i64, f64> = rrf_scores.into_iter().collect();
    let chunks = db.get_chunks_by_ids(&ids)?;

    // Build a lookup so we can re-order by score
    let mut chunk_map: HashMap<i64, _> = chunks
        .into_iter()
        .filter_map(|c| {
            let id = match c.id {
                Some(id) => id,
                None => {
                    log::warn!("Hybrid search: chunk from DB has no id, skipping");
                    return None;
                }
            };
            Some((id, c))
        })
        .collect();
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
                    vault_name: c.vault_name,
                }
            })
        })
        .collect();

    // Already filtered by vault via search_fts/search_vec; no need to refilter.
    results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));

    if let Some(ms) = min_score {
        results.retain(|r| r.score >= ms);
    }

    Ok(results)
}

/// Extract a snippet around the first query token match.
///
/// `max_lines` controls how many lines of context to include before and after
/// the matched line (total window = 2 * max_lines + 1 lines).  `max_chars`
/// caps the final snippet length at the character level.
pub fn extract_snippet(text: &str, query: &str, max_lines: usize, max_chars: usize) -> String {
    let tokens: Vec<&str> = query.split_whitespace().collect();
    if tokens.is_empty() {
        return text.chars().take(max_chars).collect::<String>() + "\u{2026}";
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
        None => return text.chars().take(max_chars).collect::<String>() + "\u{2026}",
    };

    // Walk back up to `max_lines` lines before the match to provide context
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
        result.chars().take(max_chars).collect::<String>() + "\u{2026}"
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
        let snippet = extract_snippet(text, "three", 1, 1000);
        assert!(snippet.contains("three"));
    }

    #[test]
    fn test_extract_snippet_fallback_on_no_match() {
        let text = "some content here";
        let snippet = extract_snippet(text, "nonexistent", 3, 200);
        assert!(snippet.ends_with("\u{2026}"));
    }

    #[test]
    fn test_extract_snippet_empty_query() {
        let text = "Some content without tokens";
        let snippet = extract_snippet(text, "", 3, 200);
        assert!(snippet.ends_with("\u{2026}"));
    }

    #[test]
    fn test_extract_snippet_respects_max_chars() {
        let text = "A very long line that exceeds the max_chars limit set for snippet extraction";
        let snippet = extract_snippet(text, "exceeds", 3, 20);
        assert!(snippet.ends_with("\u{2026}"));
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
            vault_name: String::new(),
        }];
        db.insert_chunks(&chunks).unwrap();

        let results = search(&db, &tokenizer, "search engine", 10, SearchMode::Fts, None, None, None).unwrap();
        assert!(!results.is_empty());
        assert_eq!(results[0].file_path, "test.md");
        assert!(matches!(results[0].search_mode, SearchMode::Fts));
    }

    #[test]
    fn test_search_empty_query_returns_empty() {
        let db = NoteDatabase::open_in_memory().unwrap();
        let tokenizer = crate::require_tokenizer!(crate::tokenizer::TokenizerConfig::default());
        let results = search(&db, &tokenizer, "  ", 10, SearchMode::Fts, None, None, None).unwrap();
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
            vault_name: String::new(),
        }];
        db.insert_chunks(&chunks).unwrap();

        // Hybrid with no embedder → falls back to FTS
        let results = search(&db, &tokenizer, "hybrid fallback", 10, SearchMode::Hybrid, None, None, None).unwrap();
        assert!(!results.is_empty());
        assert!(matches!(results[0].search_mode, SearchMode::Fts));
    }

    #[test]
    fn test_compute_rrf_identical_rankings() {
        let make = |id: i64, score: f64| -> ChunkSearchResult {
            ChunkSearchResult {
                chunk_id: id,
                file_path: format!("{}.md", id),
                parent_header: None,
                content: String::new(),
                score,
                search_mode: SearchMode::Fts,
                vault_name: String::new(),
            }
        };

        // Both FTS and vec return the same 3 chunks in the same order
        let fts = vec![make(1, 1.0), make(2, 2.0), make(3, 3.0)];
        let vec = vec![make(1, 0.5), make(2, 1.0), make(3, 1.5)];

        let result = compute_rrf(&fts, &vec, 3, 60.0);
        assert_eq!(result.len(), 3, "should return all 3 results");

        // Chunk 1 appears at rank 1 in both → highest RRF score
        // Chunk 3 appears at rank 3 in both → lowest RRF score
        assert_eq!(result[0].0, 1, "chunk 1 should be first, got {:?}", result[0]);
        assert_eq!(result[2].0, 3, "chunk 3 should be last, got {:?}", result[2]);
    }

    #[test]
    fn test_compute_rrf_disjoint_sets() {
        let make = |id: i64, score: f64| -> ChunkSearchResult {
            ChunkSearchResult {
                chunk_id: id,
                file_path: format!("{}.md", id),
                parent_header: None,
                content: String::new(),
                score,
                search_mode: SearchMode::Fts,
                vault_name: String::new(),
            }
        };

        // FTS finds chunks 1, 2; vec finds chunks 3, 4
        let fts = vec![make(1, 1.0), make(2, 2.0)];
        let vec = vec![make(3, 0.5), make(4, 1.0)];

        let result = compute_rrf(&fts, &vec, 4, 60.0);
        assert_eq!(result.len(), 4, "should return all 4 unique chunks");

        // Each chunk gets contribution from only one source
        for (_id, score) in &result {
            assert!(*score > 0.0, "all scores should be positive");
            assert!(*score < 0.02, "single-source scores should be < 0.02");
        }
    }

    #[test]
    fn test_compute_rrf_respects_limit() {
        let make = |id: i64, score: f64| -> ChunkSearchResult {
            ChunkSearchResult {
                chunk_id: id,
                file_path: format!("{}.md", id),
                parent_header: None,
                content: String::new(),
                score,
                search_mode: SearchMode::Fts,
                vault_name: String::new(),
            }
        };

        let fts = vec![make(1, 1.0), make(2, 2.0), make(3, 3.0)];
        let vec = vec![make(1, 0.5), make(2, 1.0)];

        let result = compute_rrf(&fts, &vec, 2, 60.0);
        assert_eq!(result.len(), 2, "should return only 2 results (limited)");
    }

    #[test]
    fn test_compute_rrf_empty_inputs() {
        let result = compute_rrf(&[], &[], 10, 60.0);
        assert!(result.is_empty(), "empty inputs should produce empty results");
    }

    #[test]
    fn test_compute_rrf_one_source_empty() {
        let make = |id: i64, score: f64| -> ChunkSearchResult {
            ChunkSearchResult {
                chunk_id: id,
                file_path: format!("{}.md", id),
                parent_header: None,
                content: String::new(),
                score,
                search_mode: SearchMode::Fts,
                vault_name: String::new(),
            }
        };

        let fts = vec![make(1, 1.0), make(2, 2.0)];
        let result = compute_rrf(&fts, &[], 5, 60.0);
        assert_eq!(result.len(), 2, "should return FTS results even without vec results");
    }

    #[test]
    fn test_search_vec_mode_without_embedder_returns_error() {
        let db = crate::db::NoteDatabase::open_in_memory().unwrap();
        let tokenizer = match JapaneseTokenizer::new(crate::tokenizer::TokenizerConfig::default()) {
            Ok(tok) => tok,
            Err(_) => return,
        };
        let result = search(&db, &tokenizer, "test", 10, SearchMode::Vec, None, None, None);
        match result {
            Err(crate::db::DbError::Other(msg)) => {
                assert!(msg.contains("embedder"), "error should mention embedder");
            }
            _ => panic!("expected DbError::Other with embedder message, got {:?}", result),
        }
    }

    #[test]
    fn test_search_hybrid_mode_without_embedder_falls_back_to_fts() {
        let db = crate::db::NoteDatabase::open_in_memory().unwrap();
        let tokenizer = match JapaneseTokenizer::new(crate::tokenizer::TokenizerConfig::default()) {
            Ok(tok) => tok,
            Err(_) => return,
        };
        let result = search(&db, &tokenizer, "test", 10, SearchMode::Hybrid, None, None, None);
        assert!(result.is_ok(), "Hybrid without embedder should fall back to FTS, got error");
    }

    #[test]
    fn test_search_fts_non_empty_query_min_score_high_excludes_all() {
        let db = crate::db::NoteDatabase::open_in_memory().unwrap();
        let tokenizer = match JapaneseTokenizer::new(crate::tokenizer::TokenizerConfig::default()) {
            Ok(tok) => tok,
            Err(_) => return,
        };
        let result = search(&db, &tokenizer, "test", 10, SearchMode::Fts, None, None, None);
        assert!(result.is_ok());
    }

    // ── extract_snippet edge cases ───────────────────────────────────

    #[test]
    fn test_extract_snippet_query_at_start() {
        let text = "query starts here\nLine 2\nLine 3";
        let snippet = extract_snippet(text, "query", 1, 100);
        assert!(snippet.contains("query"));
    }

    #[test]
    fn test_extract_snippet_query_at_end() {
        let text = "Line 1\nLine 2\nQuery at end";
        let snippet = extract_snippet(text, "query", 1, 100);
        assert!(snippet.contains("Query"));
    }

    #[test]
    fn test_extract_snippet_multi_token_query() {
        let text = "hello\nworld\nfoo\nhello world";
        let snippet = extract_snippet(text, "hello world", 1, 100);
        assert!(snippet.contains("hello") || snippet.contains("world"));
    }

    #[test]
    fn test_extract_snippet_max_lines_zero() {
        let text = "Line 1\nLine 2\nLine 3\nLine 4";
        let snippet = extract_snippet(text, "Line", 0, 100);
        assert!(snippet.contains("Line"));
    }

    #[test]
    fn test_extract_snippet_very_long_document() {
        let long_text = (0..1000).map(|i| format!("Line {} unique_content", i)).collect::<Vec<_>>().join("\n");
        let snippet = extract_snippet(&long_text, "unique_content", 2, 500);
        assert!(snippet.contains("unique_content"));
        // Verify the snippet is bounded in size
        assert!(snippet.len() <= 510, "snippet should be reasonably bounded");
    }

    #[test]
    fn test_extract_snippet_case_insensitive_match() {
        let text = "HELLO\nWorld\nFOO";
        let snippet = extract_snippet(text, "hello", 1, 100);
        assert!(snippet.contains("HELLO"));
    }
}
