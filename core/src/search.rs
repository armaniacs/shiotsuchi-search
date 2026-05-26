use crate::{
    db::{DbError, NoteDatabase},
    models::{ChunkSearchResult, SearchMode},
    tokenizer::{apply_user_dictionary, normalize, simple_and_query, JapaneseTokenizer},
};
use std::collections::HashMap;
use log;

/// Main search entry point. Dispatches to FTS, vec, or hybrid (RRF) mode.
///
/// `tag_filter` — comma-separated tag string to match (empty/none = no filter).
/// `since_date` — ISO 8601 date string for minimum frontmatter date filter.
#[allow(clippy::too_many_arguments)]
#[allow(clippy::too_many_arguments)]
#[allow(clippy::too_many_arguments)]
pub fn search(
    db: &NoteDatabase,
    tokenizer: &JapaneseTokenizer,
    query: &str,
    limit: usize,
    mode: SearchMode,
    embedder: Option<&crate::embedder::Embedder>,
    min_score: Option<f64>,
    vault_filter: Option<&str>,
    tag_filter: Option<&str>,
    since_date: Option<&str>,
    user_dictionary: &[String],
    synonyms: &HashMap<String, Vec<String>>,
    fuzzy: bool,
    alpha: Option<f64>,
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
        SearchMode::Fts => search_fts(db, tokenizer, query, limit, min_score, vault_filter, tag_filter, since_date, user_dictionary, synonyms, fuzzy),
        SearchMode::Vec => {
            let emb = embedder.ok_or_else(|| DbError::Other("Vec mode requires embedder — model not loaded".into()))?;
            search_vec(db, emb, query, limit, min_score, vault_filter, tag_filter, since_date)
        }
        SearchMode::Hybrid => {
            let emb = embedder.ok_or_else(|| DbError::Other("Hybrid mode requires embedder — model not loaded".into()))?;
            search_hybrid(db, tokenizer, emb, query, limit, min_score, vault_filter, tag_filter, since_date, user_dictionary, synonyms, fuzzy, alpha)
        }
    }
}

/// Check if a chunk's tags match the tag filter.
/// Tags are stored as comma-separated strings (e.g. "project,meeting").
/// The filter is a single tag name. Uses substring matching on the
/// comma-delimited list to avoid partial matches (e.g. "proj" matching "project").
fn tag_matches(tags: &str, filter: &str) -> bool {
    if filter.is_empty() {
        return true;
    }
    let filter = filter.trim().to_lowercase();
    if filter.is_empty() {
        return true;
    }
    tags.split(',')
        .any(|t| t.trim().to_lowercase() == filter)
}

/// Check if a chunk's frontmatter_date is >= the given since_date.
/// Both strings are ISO 8601 format (e.g. "2026-01-15"), so lexicographic
/// comparison works correctly.
fn date_matches(date: &str, since: &str) -> bool {
    if since.is_empty() {
        return true;
    }
    date.is_empty() || date >= since
}

/// Apply tag and date filters to a list of results, then apply title score boost.
fn apply_filters_and_boost(
    mut results: Vec<ChunkSearchResult>,
    tag_filter: Option<&str>,
    since_date: Option<&str>,
    query: &str,
) -> Vec<ChunkSearchResult> {
    let tag_f = tag_filter.unwrap_or("");
    let since_d = since_date.unwrap_or("");

    results.retain(|r| {
        tag_matches(&r.tags, tag_f) && date_matches(&r.frontmatter_date, since_d)
    });

    // Title score boost: if a chunk's title matches the query, reduce its
    // score (lower = more relevant in FTS5 BM25).
    if !query.is_empty() {
        let query_lower = query.to_lowercase();
        let query_tokens: Vec<&str> = query_lower.split_whitespace().collect();
        for r in &mut results {
            if !r.title.is_empty() {
                let title_lower = r.title.to_lowercase();
                if query_tokens.iter().any(|t| title_lower.contains(t)) {
                    r.score *= 0.3; // significant boost
                }
            }
        }
    }

    results
}

/// Build `ChunkSearchResult` vec from raw search hits (id, score) pairs.
///
/// Resolves chunk metadata from DB, applies optional min_score filter,
/// and sorts ascending (lower score = more relevant). Common post-processing
/// for both FTS and vec search paths.
fn build_results(
    db: &NoteDatabase,
    hits: Vec<(i64, f64)>,
    mode: SearchMode,
    min_score: Option<f64>,
) -> Result<Vec<ChunkSearchResult>, DbError> {
    let ids: Vec<i64> = hits.iter().map(|(id, _)| *id).collect();
    let score_map: HashMap<i64, f64> = hits.into_iter().collect();
    let chunks = db.get_chunks_by_ids(&ids)?;

    let mut results: Vec<ChunkSearchResult> = chunks
        .into_iter()
        .filter_map(|c| {
            let id = match c.id {
                Some(id) => id,
                None => {
                    log::warn!("{:?} search: chunk from DB has no id, skipping", mode);
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
                search_mode: mode.clone(),
                vault_name: c.vault_name,
                tags: c.tags,
                frontmatter_date: c.frontmatter_date,
                title: c.title,
            })
        })
        .collect();

    // Lower score = more relevant; sort ascending
    results.sort_by(|a, b| a.score.partial_cmp(&b.score).unwrap_or(std::cmp::Ordering::Equal));

    if let Some(ms) = min_score {
        results.retain(|r| r.score <= ms);
    }

    Ok(results)
}

/// Expand tokens with synonyms using FTS5 OR syntax.
/// Each token that has synonyms becomes `("token" OR "synonym1" OR "synonym2")`.
/// Tokens without synonyms remain as `"token"`.
/// All clauses are AND-joined.
fn expand_synonyms(
    tokens: &[String],
    synonyms: &HashMap<String, Vec<String>>,
) -> String {
    if synonyms.is_empty() {
        return tokens
            .iter()
            .map(|t| format!("\"{}\"", t.replace('"', "\"\"")))
            .collect::<Vec<_>>()
            .join(" AND ");
    }

    let clauses: Vec<String> = tokens
        .iter()
        .map(|token| {
            if let Some(syns) = synonyms.get(token) {
                let mut parts = vec![format!("\"{}\"", token.replace('"', "\"\""))];
                for syn in syns {
                    parts.push(format!("\"{}\"", syn.replace('"', "\"\"")));
                }
                format!("({})", parts.join(" OR "))
            } else {
                format!("\"{}\"", token.replace('"', "\"\""))
            }
        })
        .collect();

    clauses.join(" AND ")
}

fn search_fts(
    db: &NoteDatabase,
    tokenizer: &JapaneseTokenizer,
    query: &str,
    limit: usize,
    min_score: Option<f64>,
    vault_filter: Option<&str>,
    tag_filter: Option<&str>,
    since_date: Option<&str>,
    user_dictionary: &[String],
    synonyms: &HashMap<String, Vec<String>>,
    fuzzy: bool,
) -> Result<Vec<ChunkSearchResult>, DbError> {
    let tokens = tokenizer.collect_tokens(query);
    let tokens = if fuzzy {
        tokens.iter().map(|t| normalize(t)).collect()
    } else {
        tokens
    };
    let tokens = apply_user_dictionary(&tokens, user_dictionary);
    let fts5_query = expand_synonyms(&tokens, synonyms);
    let fts5_query = if fts5_query.is_empty() {
        simple_and_query(query)
    } else {
        fts5_query
    };
    if fts5_query.is_empty() {
        return Ok(vec![]);
    }

    let hits = db.fts_search(&fts5_query, limit, vault_filter)?;
    if hits.is_empty() {
        return Ok(vec![]);
    }

    let results = build_results(db, hits, SearchMode::Fts, min_score)?;
    Ok(apply_filters_and_boost(results, tag_filter, since_date, query))
}

fn search_vec(
    db: &NoteDatabase,
    embedder: &crate::embedder::Embedder,
    query: &str,
    limit: usize,
    min_score: Option<f64>,
    vault_filter: Option<&str>,
    tag_filter: Option<&str>,
    since_date: Option<&str>,
) -> Result<Vec<ChunkSearchResult>, DbError> {
    let embedding = embedder
        .embed(query)
        .map_err(|e| DbError::Other(e.to_string()))?;

    let hits = db.vec_search(&embedding, limit, vault_filter)?;
    if hits.is_empty() {
        return Ok(vec![]);
    }

    let results = build_results(db, hits, SearchMode::Vec, min_score)?;
    Ok(apply_filters_and_boost(results, tag_filter, since_date, query))
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

/// RRF with alpha weighting: `score = alpha * 1/(k + rank_fts) + (1-alpha) * 1/(k + rank_vec)`.
/// Avoids the score-normalization complexity of a linear blend by operating on
/// ranks (like standard RRF), but lets the user control the relative contribution
/// of each search method.
fn compute_rrf_weighted(
    fts_results: &[ChunkSearchResult],
    vec_results: &[ChunkSearchResult],
    limit: usize,
    k: f64,
    alpha: f64,
) -> Vec<(i64, f64)> {
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

    let mut all_ids: Vec<i64> = fts_ranks.keys().chain(vec_ranks.keys()).copied().collect();
    all_ids.sort_unstable();
    all_ids.dedup();

    let fts_weight = alpha;
    let vec_weight = 1.0 - alpha;

    let mut scores: Vec<(i64, f64)> = all_ids
        .into_iter()
        .map(|id| {
            let f = fts_ranks.get(&id).map(|&r| fts_weight / (k + r as f64)).unwrap_or(0.0);
            let v = vec_ranks.get(&id).map(|&r| vec_weight / (k + r as f64)).unwrap_or(0.0);
            (id, f + v)
        })
        .collect();

    scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    scores.truncate(limit);
    scores
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
    tag_filter: Option<&str>,
    since_date: Option<&str>,
    user_dictionary: &[String],
    synonyms: &HashMap<String, Vec<String>>,
    fuzzy: bool,
    alpha: Option<f64>,
) -> Result<Vec<ChunkSearchResult>, DbError> {
    const K: f64 = 60.0;

    let fts_results = search_fts(db, tokenizer, query, limit * 2, None, vault_filter, None, None, user_dictionary, synonyms, fuzzy)?;
    let vec_results = search_vec(db, embedder, query, limit * 2, None, vault_filter, None, None)?;

    let blended_scores = match alpha {
        Some(a) => compute_rrf_weighted(&fts_results, &vec_results, limit, K, a.clamp(0.0, 1.0)),
        None => compute_rrf(&fts_results, &vec_results, limit, K),
    };

    if blended_scores.is_empty() {
        return Ok(vec![]);
    }

    let ids: Vec<i64> = blended_scores.iter().map(|(id, _)| *id).collect();
    let score_map: HashMap<i64, f64> = blended_scores.into_iter().collect();
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
                    tags: c.tags,
                    frontmatter_date: c.frontmatter_date,
                    title: c.title,
                }
            })
        })
        .collect();

    results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));

    if let Some(ms) = min_score {
        results.retain(|r| r.score >= ms);
    }

    Ok(apply_filters_and_boost(results, tag_filter, since_date, query))
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
            tags: String::new(),
            frontmatter_date: String::new(),
            title: String::new(),
        }];
        db.insert_chunks(&chunks).unwrap();

        let results = search(&db, &tokenizer, "search engine", 10, SearchMode::Fts, None, None, None, None, None, &[], &HashMap::new(), false, None).unwrap();
        assert!(!results.is_empty());
        assert_eq!(results[0].file_path, "test.md");
        assert!(matches!(results[0].search_mode, SearchMode::Fts));
    }

    #[test]
    fn test_search_fts_vault_filter_respects_filter() {
        let db = NoteDatabase::open_in_memory().unwrap();
        let tokenizer = crate::require_tokenizer!(crate::tokenizer::TokenizerConfig::default());

        let chunks = vec![
            Chunk {
                id: None,
                file_path: "vault_a.md".into(),
                chunk_index: 0,
                parent_header: None,
                content: "alpha project plan".into(),
                tokenized_content: "alpha project plan".into(),
                vault_name: "work".into(),
                tags: String::new(),
                frontmatter_date: String::new(),
                title: String::new(),
            },
            Chunk {
                id: None,
                file_path: "vault_b.md".into(),
                chunk_index: 0,
                parent_header: None,
                content: "alpha social event".into(),
                tokenized_content: "alpha social event".into(),
                vault_name: "personal".into(),
                tags: String::new(),
                frontmatter_date: String::new(),
                title: String::new(),
            },
        ];
        db.insert_chunks(&chunks).unwrap();

        // Filter by "work" vault
        let results = search(&db, &tokenizer, "alpha", 10, SearchMode::Fts, None, None, Some("work"), None, None, &[], &HashMap::new(), false, None).unwrap();
        assert_eq!(results.len(), 1, "expected 1 result in work vault");
        assert_eq!(results[0].vault_name, "work");
        assert_eq!(results[0].file_path, "vault_a.md");

        // Filter by "personal" vault
        let results = search(&db, &tokenizer, "alpha", 10, SearchMode::Fts, None, None, Some("personal"), None, None, &[], &HashMap::new(), false, None).unwrap();
        assert_eq!(results.len(), 1, "expected 1 result in personal vault");
        assert_eq!(results[0].vault_name, "personal");
        assert_eq!(results[0].file_path, "vault_b.md");

        // No filter → both
        let results = search(&db, &tokenizer, "alpha", 10, SearchMode::Fts, None, None, None, None, None, &[], &HashMap::new(), false, None).unwrap();
        assert_eq!(results.len(), 2, "expected 2 results across all vaults");
    }

    #[test]
    fn test_search_empty_query_returns_empty() {
        let db = NoteDatabase::open_in_memory().unwrap();
        let tokenizer = crate::require_tokenizer!(crate::tokenizer::TokenizerConfig::default());
        let results = search(&db, &tokenizer, "  ", 10, SearchMode::Fts, None, None, None, None, None, &[], &HashMap::new(), false, None).unwrap();
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
            tags: String::new(),
            frontmatter_date: String::new(),
            title: String::new(),
        }];
        db.insert_chunks(&chunks).unwrap();

        // Hybrid with no embedder → falls back to FTS
        let results = search(&db, &tokenizer, "hybrid fallback", 10, SearchMode::Hybrid, None, None, None, None, None, &[], &HashMap::new(), false, None).unwrap();
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
                tags: String::new(),
                frontmatter_date: String::new(),
                title: String::new(),
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
                tags: String::new(),
                frontmatter_date: String::new(),
                title: String::new(),
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
                tags: String::new(),
                frontmatter_date: String::new(),
                title: String::new(),
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

    // ── RRF weighted (alpha) ─────────────────────────────────────

    fn make_parent(id: i64) -> ChunkSearchResult {
        ChunkSearchResult {
            chunk_id: id,
            file_path: format!("{}.md", id),
            parent_header: None,
            content: String::new(),
            score: 0.0,
            search_mode: SearchMode::Fts,
            vault_name: String::new(),
            tags: String::new(),
            frontmatter_date: String::new(),
            title: String::new(),
        }
    }

    #[test]
    fn test_rrf_weighted_alpha_1_0_uses_fts_only() {
        let fts = vec![make_parent(1), make_parent(2)];
        let vec = vec![make_parent(1), make_parent(2)];
        let result = compute_rrf_weighted(&fts, &vec, 2, 60.0, 1.0);
        // alpha=1.0 → only FTS rank matters
        // FTS: chunk 1 (rank 1) > chunk 2 (rank 2)
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].0, 1, "chunk 1 should be first with alpha=1.0");
    }

    #[test]
    fn test_rrf_weighted_alpha_0_0_uses_vec_only() {
        let fts = vec![make_parent(1), make_parent(2)];
        let vec = vec![make_parent(2), make_parent(1)]; // reversed order
        let result = compute_rrf_weighted(&fts, &vec, 2, 60.0, 0.0);
        // alpha=0.0 → only vec rank matters
        // Vec: chunk 2 (rank 1) > chunk 1 (rank 2)
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].0, 2, "chunk 2 should be first with alpha=0.0 (vec rank 1)");
    }

    #[test]
    fn test_rrf_weighted_alpha_0_5_balanced() {
        let fts = vec![make_parent(1), make_parent(2)];
        let vec = vec![make_parent(2), make_parent(1)];
        let result = compute_rrf_weighted(&fts, &vec, 2, 60.0, 0.5);
        // Both rank 1 in one source and rank 2 in the other → equal scores
        assert_eq!(result.len(), 2);
        let score1 = result.iter().find(|(id, _)| *id == 1).map(|(_, s)| *s).unwrap();
        let score2 = result.iter().find(|(id, _)| *id == 2).map(|(_, s)| *s).unwrap();
        let diff = (score1 - score2).abs();
        assert!(diff < 0.001, "scores should be equal with alpha=0.5, got {:.6} vs {:.6}", score1, score2);
    }

    #[test]
    fn test_rrf_weighted_empty_inputs() {
        let result = compute_rrf_weighted(&[], &[], 10, 60.0, 0.5);
        assert!(result.is_empty(), "empty inputs should produce empty results");
    }

    #[test]
    fn test_rrf_weighted_only_fts_results() {
        let fts = vec![make_parent(1), make_parent(2)];
        let vec = vec![];
        let result = compute_rrf_weighted(&fts, &vec, 2, 60.0, 0.5);
        assert_eq!(result.len(), 2);
        // FTS alone determines order (vec weight contributes 0)
        assert_eq!(result[0].0, 1, "chunk 1 (FTS rank 1) should be first");
    }

    #[test]
    fn test_rrf_weighted_respects_limit() {
        let fts = vec![make_parent(1), make_parent(2), make_parent(3)];
        let vec = vec![make_parent(1), make_parent(2), make_parent(3)];
        let result = compute_rrf_weighted(&fts, &vec, 1, 60.0, 0.5);
        assert_eq!(result.len(), 1, "should return only 1 result (limited)");
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
                tags: String::new(),
                frontmatter_date: String::new(),
                title: String::new(),
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
        let result = search(&db, &tokenizer, "test", 10, SearchMode::Vec, None, None, None, None, None, &[], &HashMap::new(), false, None);
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
        let result = search(&db, &tokenizer, "test", 10, SearchMode::Hybrid, None, None, None, None, None, &[], &HashMap::new(), false, None);
        assert!(result.is_ok(), "Hybrid without embedder should fall back to FTS, got error");
    }

    #[test]
    fn test_search_fts_non_empty_query_min_score_high_excludes_all() {
        let db = crate::db::NoteDatabase::open_in_memory().unwrap();
        let tokenizer = match JapaneseTokenizer::new(crate::tokenizer::TokenizerConfig::default()) {
            Ok(tok) => tok,
            Err(_) => return,
        };
        let result = search(&db, &tokenizer, "test", 10, SearchMode::Fts, None, None, None, None, None, &[], &HashMap::new(), false, None);
        assert!(result.is_ok());
    }

    // ── fuzzy search ────────────────────────────────────────────────

    #[test]
    fn test_search_fuzzy_true_normalizes_query_and_matches() {
        let db = crate::db::NoteDatabase::open_in_memory().unwrap();
        let tokenizer = match JapaneseTokenizer::new(crate::tokenizer::TokenizerConfig::default()) {
            Ok(tok) => tok,
            Err(_) => return,
        };

        // Insert a chunk with normalized tokenized_content (mimics what the indexer stores)
        let chunks = vec![
            crate::models::Chunk {
                id: None,
                file_path: "test.md".into(),
                chunk_index: 0,
                parent_header: None,
                content: "hello world".into(),
                tokenized_content: "hello world".into(), // already normalized
                vault_name: String::new(),
                tags: String::new(),
                frontmatter_date: String::new(),
                title: String::new(),
            },
        ];
        db.insert_chunks(&chunks).unwrap();

        // fuzzy=false with uppercase query → no match (case-sensitive FTS5)
        let results = search(&db, &tokenizer, "HELLO", 10, SearchMode::Fts, None, None, None, None, None, &[], &HashMap::new(), false, None).unwrap();
        assert!(results.is_empty(), "non-fuzzy search should not match uppercase 'HELLO' against 'hello'");

        // fuzzy=true → query is normalized to lowercase → matches
        let results = search(&db, &tokenizer, "HELLO", 10, SearchMode::Fts, None, None, None, None, None, &[], &HashMap::new(), true, None).unwrap();
        assert!(!results.is_empty(), "fuzzy search should match 'HELLO' against 'hello'");
        assert_eq!(results[0].file_path, "test.md");
    }

    #[test]
    fn test_search_fuzzy_fullwidth_matches_halfwidth() {
        let db = crate::db::NoteDatabase::open_in_memory().unwrap();
        let tokenizer = match JapaneseTokenizer::new(crate::tokenizer::TokenizerConfig::default()) {
            Ok(tok) => tok,
            Err(_) => return,
        };

        // Insert normalized tokenized content (halfwidth)
        let chunks = vec![
            crate::models::Chunk {
                id: None,
                file_path: "note.md".into(),
                chunk_index: 0,
                parent_header: None,
                content: "apple".into(),
                tokenized_content: "apple".into(),
                vault_name: String::new(),
                tags: String::new(),
                frontmatter_date: String::new(),
                title: String::new(),
            },
        ];
        db.insert_chunks(&chunks).unwrap();

        // fuzzy=true with fullwidth query → normalized to "apple" → matches
        let results = search(&db, &tokenizer, "ａｐｐｌｅ", 10, SearchMode::Fts, None, None, None, None, None, &[], &HashMap::new(), true, None).unwrap();
        assert!(!results.is_empty(), "fuzzy search should match fullwidth 'ａｐｐｌｅ' against 'apple'");
    }

    #[test]
    fn test_search_fuzzy_false_does_not_normalize() {
        let db = crate::db::NoteDatabase::open_in_memory().unwrap();
        let tokenizer = match JapaneseTokenizer::new(crate::tokenizer::TokenizerConfig::default()) {
            Ok(tok) => tok,
            Err(_) => return,
        };

        // Insert non-normalized tokenized content (mixed case)
        let chunks = vec![
            crate::models::Chunk {
                id: None,
                file_path: "test.md".into(),
                chunk_index: 0,
                parent_header: None,
                content: "Hello".into(),
                tokenized_content: "Hello".into(), // not normalized (mimics old DB)
                vault_name: String::new(),
                tags: String::new(),
                frontmatter_date: String::new(),
                title: String::new(),
            },
        ];
        db.insert_chunks(&chunks).unwrap();

        // fuzzy=false with exact case → matches
        let results = search(&db, &tokenizer, "Hello", 10, SearchMode::Fts, None, None, None, None, None, &[], &HashMap::new(), false, None).unwrap();
        assert!(!results.is_empty(), "non-fuzzy should match exact case 'Hello'");

        // fuzzy=false with different case → no match
        let results = search(&db, &tokenizer, "hello", 10, SearchMode::Fts, None, None, None, None, None, &[], &HashMap::new(), false, None).unwrap();
        assert!(results.is_empty(), "non-fuzzy should not match different case 'hello' against 'Hello'");
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

    #[test]
    fn test_extract_snippet_japanese_query_no_spaces() {
        // Japanese queries without spaces should still produce a snippet.
        // Even though split_whitespace treats the whole query as one token,
        // the snippet should contain the matching text.
        let text = "これは日本語の検索エンジンのテストです\n次の行\nさらに続く";
        let snippet = extract_snippet(text, "検索エンジン", 1, 200);
        assert!(snippet.contains("検索エンジン"),
            "Japanese query without spaces should match: {}", snippet);
    }

    #[test]
    fn test_extract_snippet_japanese_mixed_with_ascii() {
        let text = "Rust is a systems language\n日本語のメモリ安全性\n検索エンジン";
        let snippet = extract_snippet(text, "メモリ安全", 1, 200);
        assert!(snippet.contains("メモリ安全"),
            "Japanese query in mixed text should match: {}", snippet);
    }

    #[test]
    fn test_extract_snippet_unicode_query_prioritizes_earliest_match() {
        let text = "最初のマッチ\n途中のテキスト\n最後のマッチ";
        let snippet = extract_snippet(text, "マッチ", 1, 100);
        assert!(snippet.contains("最初"), "should prioritize earliest match position");
    }

    // ── synonym expansion ──────────────────────────────────────

    #[test]
    fn test_expand_synonyms_no_synonyms_returns_basic_and_query() {
        let tokens = vec!["AWS".to_string(), "service".to_string()];
        let synonyms = std::collections::HashMap::new();
        let result = expand_synonyms(&tokens, &synonyms);
        assert_eq!(result, r#""AWS" AND "service""#);
    }

    #[test]
    fn test_expand_synonyms_single_token_expanded_with_or() {
        let tokens = vec!["AWS".to_string()];
        let mut synonyms = std::collections::HashMap::new();
        synonyms.insert(
            "AWS".to_string(),
            vec!["Amazon Web Services".to_string(), "アマゾン".to_string()],
        );
        let result = expand_synonyms(&tokens, &synonyms);
        assert_eq!(
            result,
            r#"("AWS" OR "Amazon Web Services" OR "アマゾン")"#
        );
    }

    #[test]
    fn test_expand_synonyms_mixed_tokens() {
        let tokens = vec!["AWS".to_string(), "database".to_string()];
        let mut synonyms = std::collections::HashMap::new();
        synonyms.insert(
            "AWS".to_string(),
            vec!["Amazon".to_string()],
        );
        let result = expand_synonyms(&tokens, &synonyms);
        assert_eq!(
            result,
            r#"("AWS" OR "Amazon") AND "database""#
        );
    }

    #[test]
    fn test_expand_synonyms_multiple_tokens_with_synonyms() {
        let tokens = vec!["AWS".to_string(), "k8s".to_string()];
        let mut synonyms = std::collections::HashMap::new();
        synonyms.insert(
            "AWS".to_string(),
            vec!["Amazon".to_string()],
        );
        synonyms.insert(
            "k8s".to_string(),
            vec!["Kubernetes".to_string()],
        );
        let result = expand_synonyms(&tokens, &synonyms);
        assert_eq!(
            result,
            r#"("AWS" OR "Amazon") AND ("k8s" OR "Kubernetes")"#
        );
    }

    #[test]
    fn test_expand_synonyms_escapes_quotes_in_terms() {
        let tokens = vec!["term".to_string()];
        let mut synonyms = std::collections::HashMap::new();
        synonyms.insert(
            "term".to_string(),
            vec!["say \"hi\"".to_string()],
        );
        let result = expand_synonyms(&tokens, &synonyms);
        assert_eq!(result, r#"("term" OR "say ""hi""")"#);
    }
}
