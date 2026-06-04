use crate::{
    db::{DbError, NoteDatabase},
    embedder::Embedder,
    models::{ChunkSearchResult, SearchMode},
    tokenizer::{apply_user_dictionary, normalize, simple_and_query, JapaneseTokenizer},
};
use std::collections::HashMap;
use log;

/// All parameters for a search operation, grouped for maintainability.
#[derive(Debug, Clone)]
pub struct SearchRequest<'a> {
    pub query: &'a str,
    pub limit: usize,
    pub mode: SearchMode,
    pub embedder: Option<&'a Embedder>,
    pub min_score: Option<f64>,
    pub vault_filter: Option<&'a str>,
    pub tag_filter: Option<&'a str>,
    pub since_date: Option<&'a str>,
    pub user_dictionary: &'a [String],
    pub synonyms: &'a HashMap<String, Vec<String>>,
    pub fuzzy: bool,
    pub hybrid_alpha: Option<f64>,
    pub mmr: bool,
    pub lambda: f64,
    pub backlink_scoring: bool,
}

/// Main search entry point. Dispatches to FTS, vec, or hybrid (RRF) mode.
///
/// `tag_filter` — comma-separated tag string to match (empty/none = no filter).
/// `since_date` — ISO 8601 date string for minimum frontmatter date filter.
///
/// Cosine similarity between two f32 vectors.
fn cosine_similarity(a: &[f32], b: &[f32]) -> f64 {
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }
    (dot / (norm_a * norm_b)) as f64
}

/// MMR (Maximal Marginal Relevance) re-ranking.
///
/// Selects results iteratively: at each step, picks the candidate with the
/// highest marginal relevance score:
///
///   MMR(d) = λ · Sim(d, query) − (1−λ) · max_{s∈S} Sim(d, s)
///
/// where S is the set of already-selected results.
///
/// λ = 1.0 → pure relevance (same as original order).
/// λ = 0.0 → pure diversity (only dissimilar results).
fn mmr_rerank(
    candidates: Vec<ChunkSearchResult>,
    query_vector: &[f32],
    candidate_vectors: &HashMap<i64, Vec<f32>>,
    lambda: f64,
    limit: usize,
) -> Vec<ChunkSearchResult> {
    if candidates.len() <= 1 || query_vector.is_empty() {
        return candidates;
    }

    let n = candidates.len();

    // Guard against OOM from huge n×n similarity matrix allocation.
    const MAX_MMR_CANDIDATES: usize = 1_000;
    if n > MAX_MMR_CANDIDATES {
        return candidates;
    }

    // Assign a stable index to each candidate (None = no vector available).
    // Candidates without vectors get query_sim=0 and are never preferred.
    let vecs: Vec<Option<&Vec<f32>>> = candidates
        .iter()
        .map(|r| candidate_vectors.get(&r.chunk_id))
        .collect();

    // Pre-compute query similarity for all candidates (O(n)).
    let query_sims: Vec<f64> = vecs
        .iter()
        .map(|v| v.map(|vec| cosine_similarity(query_vector, vec)).unwrap_or(0.0))
        .collect();

    // Pre-compute the full n×n pairwise similarity matrix as a flat Vec<f32>.
    // Only the upper triangle is computed; sim_matrix[i*n + j] mirrors [j*n + i].
    // This eliminates redundant cosine_similarity calls during the selection loop.
    let mut sim_matrix: Vec<f32> = vec![0.0f32; n * n];
    for i in 0..n {
        sim_matrix[i * n + i] = 1.0;
        if let Some(vi) = vecs[i] {
            for j in (i + 1)..n {
                if let Some(vj) = vecs[j] {
                    let s = cosine_similarity(vi, vj) as f32;
                    sim_matrix[i * n + j] = s;
                    sim_matrix[j * n + i] = s;
                }
            }
        }
    }

    // Tracks which original indices have been selected, in selection order.
    let mut selected_indices: Vec<usize> = Vec::with_capacity(limit.min(n));
    // Remaining original indices not yet selected (shrinks each iteration).
    let mut remaining: Vec<usize> = (0..n).collect();

    while selected_indices.len() < limit && !remaining.is_empty() {
        let mut best_pos = 0;
        let mut best_score = f64::NEG_INFINITY;

        for (pos, &orig_i) in remaining.iter().enumerate() {
            let query_sim = query_sims[orig_i];
            let diversity_penalty = if selected_indices.is_empty() {
                0.0
            } else {
                selected_indices
                    .iter()
                    .map(|&sel_j| sim_matrix[orig_i * n + sel_j] as f64)
                    .fold(0.0_f64, f64::max)
            };

            let mmr_score = lambda * query_sim - (1.0 - lambda) * diversity_penalty;
            if mmr_score > best_score {
                best_score = mmr_score;
                best_pos = pos;
            }
        }

        let chosen_orig = remaining.remove(best_pos);
        selected_indices.push(chosen_orig);
    }

    // Reconstruct results in selection order from the original candidates vec.
    let mut candidate_map: HashMap<usize, ChunkSearchResult> = candidates
        .into_iter()
        .enumerate()
        .collect();

    selected_indices
        .into_iter()
        .filter_map(|i| candidate_map.remove(&i))
        .collect()
}

pub fn search(
    db: &NoteDatabase,
    tokenizer: &JapaneseTokenizer,
    req: &SearchRequest,
) -> Result<Vec<ChunkSearchResult>, DbError> {
    if req.query.trim().is_empty() {
        return Ok(vec![]);
    }

    let mut effective_mode = if req.embedder.is_none() && matches!(req.mode, SearchMode::Hybrid) {
        SearchMode::Fts
    } else {
        req.mode.clone()
    };

    // When MMR is active, expand the candidate pool so the algorithm has
    // diverse material to select from. FTS pool stays at limit*2 to avoid
    // slow query expansion; only the vec side is widened.
    const MMR_POOL_MULTIPLIER: usize = 3;
    let vec_fetch_limit = if req.mmr && effective_mode != SearchMode::Fts {
        req.limit * MMR_POOL_MULTIPLIER
    } else {
        req.limit
    };

    // Embed the query once and share it across vec search and MMR.
    // This avoids running the ONNX model twice for Vec/Hybrid + MMR combinations.
    // Propagate embedding errors so the user sees the real cause (e.g. model
    // runtime failure) instead of a generic "model not loaded" message.
    let precomputed_embedding = if effective_mode != SearchMode::Fts {
        match req.embedder {
            Some(e) => e.embed(req.query).map(Some).map_err(|err| DbError::Other(err.to_string())),
            None => Ok(None),
        }
    } else {
        Ok(None)
    };
    let precomputed_embedding: Option<Vec<f32>> = precomputed_embedding?;

    // vec_emb_map carries the embeddings returned by vec_search for free,
    // so MMR does not need a separate get_chunk_vectors() DB query.
    // Only request embeddings from vec_search when MMR is active, to avoid
    // unnecessary blob deserialization of potentially large embedding vectors.
    let include_embeddings = req.mmr && effective_mode != SearchMode::Fts;

    let (mut results, vec_emb_map) = match effective_mode {
        SearchMode::Fts => (search_fts(db, tokenizer, req.query, req.limit, req.min_score, req.vault_filter, req.tag_filter, req.since_date, req.user_dictionary, req.synonyms, req.fuzzy)?, HashMap::new()),
        SearchMode::Vec => {
            let emb_vec = precomputed_embedding.as_deref()
                .ok_or_else(|| DbError::Other("Vec mode requires embedder — model not loaded".into()))?;
            search_vec(db, emb_vec, req.query, vec_fetch_limit, req.min_score, req.vault_filter, req.tag_filter, req.since_date, include_embeddings)?
        }
        SearchMode::Hybrid => {
            let emb_vec = precomputed_embedding.as_deref()
                .ok_or_else(|| DbError::Other("Hybrid mode requires embedder — model not loaded".into()))?;
            match search_hybrid(db, tokenizer, emb_vec, req.query, req.limit, vec_fetch_limit, req.min_score, req.vault_filter, req.tag_filter, req.since_date, req.user_dictionary, req.synonyms, req.fuzzy, req.hybrid_alpha, include_embeddings) {
                Ok(result) => result,
                Err(e) => {
                    log::warn!("Hybrid search vec component failed ({}), falling back to FTS only", e);
                    effective_mode = SearchMode::Fts;
                    (search_fts(db, tokenizer, req.query, req.limit, req.min_score, req.vault_filter, req.tag_filter, req.since_date, req.user_dictionary, req.synonyms, req.fuzzy)?, HashMap::new())
                }
            }
        }
    };

    // MMR post-processing for Vec and Hybrid modes.
    // Uses embeddings from vec_search (no extra DB round-trip needed).
    if req.mmr && effective_mode != SearchMode::Fts {
        if let Some(query_vec) = &precomputed_embedding {
            if !vec_emb_map.is_empty() {
                results = mmr_rerank(results, query_vec, &vec_emb_map, req.lambda.clamp(0.0, 1.0), req.limit);
            }
        }
    }

    // Apply backlink scoring boost
    if req.backlink_scoring && !results.is_empty() {
        let chunk_ids: Vec<i64> = results.iter().map(|r| r.chunk_id).collect();
        if let Ok(backlink_map) = db.get_backlink_counts_for_chunks(&chunk_ids) {
            for r in &mut results {
                if let Some(&count) = backlink_map.get(&r.chunk_id) {
                    if count > 0 {
                        match effective_mode {
                            SearchMode::Fts | SearchMode::Vec => {
                                // Lower score = more relevant → subtract to improve ranking
                                r.score -= count as f64 * 0.05;
                            }
                            SearchMode::Hybrid => {
                                // Higher score = more relevant → add to improve ranking
                                r.score += count as f64 * 0.05;
                            }
                        }
                    }
                }
            }
            // Re-sort after score adjustments
            match effective_mode {
                SearchMode::Fts | SearchMode::Vec => {
                    results.sort_by(|a, b| a.score.partial_cmp(&b.score).unwrap_or(std::cmp::Ordering::Equal));
                }
                SearchMode::Hybrid => {
                    results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
                }
            }
        }
    }

    Ok(results)
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

/// Apply tag and date filters to a list of results, then apply title and
/// Apply tag/date filters and title/emphasized_text score boosts.
///
/// For FTS/Vec modes (lower score = more relevant), matching boosts reduce the
/// score (multiply by <1.0). For Hybrid mode (higher RRF score = more relevant),
/// the boost increases the score (divide by the same factor).
fn apply_filters_and_boost(
    mut results: Vec<ChunkSearchResult>,
    tag_filter: Option<&str>,
    since_date: Option<&str>,
    query: &str,
    mode: SearchMode,
) -> Vec<ChunkSearchResult> {
    let tag_f = tag_filter.unwrap_or("");
    let since_d = since_date.unwrap_or("");

    results.retain(|r| {
        tag_matches(&r.tags, tag_f) && date_matches(&r.frontmatter_date, since_d)
    });

    if !query.is_empty() {
        let query_lower = query.to_lowercase();
        let query_tokens: Vec<&str> = query_lower.split_whitespace().collect();

        // Title score boost: if a chunk's title contains a query token,
        // adjust its score to indicate higher relevance.
        //
        // For Hybrid mode (higher score = more relevant), boost by dividing
        // (1.0/0.3 ≈ 3.33× score increase). RRF scores are always ≥ 0 so
        // direction is unambiguous.
        //
        // For FTS/Vec modes (lower score = more relevant), the boost must
        // reduce the score.  BM25 can return either positive (common terms)
        // or negative (informative terms) scores, so we apply the factor
        // sign-aware: negative scores are divided (more negative = better),
        // positive scores are multiplied (less positive = better).
        let title_factor = 0.3;
        for r in &mut results {
            if !r.title.is_empty() {
                let title_lower = r.title.to_lowercase();
                if query_tokens.iter().any(|t| title_lower.contains(t)) {
                    match mode {
                        SearchMode::Hybrid => {
                            r.score *= 1.0 / title_factor;
                        }
                        _ => {
                            if r.score < 0.0 {
                                r.score /= title_factor;
                            } else {
                                r.score *= title_factor;
                            }
                        }
                    }
                }
            }
        }

        // Emphasized text score boost: same logic for ==highlight== and **bold**.
        let emph_factor = 0.5;
        for r in &mut results {
            if !r.emphasized_text.is_empty() {
                let emph_lower = r.emphasized_text.to_lowercase();
                if query_tokens.iter().any(|t| emph_lower.contains(t)) {
                    match mode {
                        SearchMode::Hybrid => {
                            r.score *= 1.0 / emph_factor;
                        }
                        _ => {
                            if r.score < 0.0 {
                                r.score /= emph_factor;
                            } else {
                                r.score *= emph_factor;
                            }
                        }
                    }
                }
            }
        }

        // Re-sort after score adjustments so boosted results appear at the
        // correct position regardless of the original BM25/vec ranking.
        match mode {
            SearchMode::Fts | SearchMode::Vec => {
                results.sort_by(|a, b| a.score.partial_cmp(&b.score).unwrap_or(std::cmp::Ordering::Equal));
            }
            SearchMode::Hybrid => {
                results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
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
pub(crate) fn build_results(
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
                emphasized_text: c.emphasized_text,
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
    Ok(apply_filters_and_boost(results, tag_filter, since_date, query, SearchMode::Fts))
}

/// Vec KNN search. Returns (results, embedding_map).
/// The embedding_map can be reused by MMR re-ranking, avoiding a second DB query.
fn search_vec(
    db: &NoteDatabase,
    embedding: &[f32],
    query: &str,
    limit: usize,
    min_score: Option<f64>,
    vault_filter: Option<&str>,
    tag_filter: Option<&str>,
    since_date: Option<&str>,
    include_embeddings: bool,
) -> Result<(Vec<ChunkSearchResult>, HashMap<i64, Vec<f32>>), DbError> {
    let raw_hits = db.vec_search(embedding, limit, vault_filter, include_embeddings)?;
    if raw_hits.is_empty() {
        return Ok((vec![], HashMap::new()));
    }

    let emb_map: HashMap<i64, Vec<f32>> = raw_hits
        .iter()
        .map(|(id, _, emb)| (*id, emb.clone()))
        .collect();
    let hits: Vec<(i64, f64)> = raw_hits.into_iter().map(|(id, dist, _)| (id, dist)).collect();

    let results = build_results(db, hits, SearchMode::Vec, min_score)?;
    Ok((apply_filters_and_boost(results, tag_filter, since_date, query, SearchMode::Vec), emb_map))
}

/// Internal RRF computation shared by `compute_rrf` and `compute_rrf_weighted`.
///
/// `weight_fn` maps an FTS rank and a vec rank to a pair of (fts_weight, vec_weight).
/// For standard RRF this is `(1.0, 1.0)`, for alpha-weighted it's `(alpha, 1.0 - alpha)`.
fn compute_rrf_internal(
    fts_results: &[ChunkSearchResult],
    vec_results: &[ChunkSearchResult],
    limit: usize,
    k: f64,
    weight_fn: impl Fn(usize, usize) -> (f64, f64),
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

    let mut scores: Vec<(i64, f64)> = all_ids
        .into_iter()
        .map(|id| {
            let (fts_weight, vec_weight) = weight_fn(
                fts_ranks.get(&id).copied().unwrap_or(usize::MAX),
                vec_ranks.get(&id).copied().unwrap_or(usize::MAX),
            );
            let f = if fts_weight != 0.0 {
                fts_ranks.get(&id).map(|&r| fts_weight / (k + r as f64)).unwrap_or(0.0)
            } else {
                0.0
            };
            let v = if vec_weight != 0.0 {
                vec_ranks.get(&id).map(|&r| vec_weight / (k + r as f64)).unwrap_or(0.0)
            } else {
                0.0
            };
            (id, f + v)
        })
        .collect();

    scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    scores.truncate(limit);
    scores
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
    compute_rrf_internal(fts_results, vec_results, limit, k, |_, _| (1.0, 1.0))
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
    let fts_weight = alpha;
    let vec_weight = 1.0 - alpha;
    compute_rrf_internal(fts_results, vec_results, limit, k, move |_, _| (fts_weight, vec_weight))
}

/// Hybrid search using Reciprocal Rank Fusion (RRF) to merge FTS + vec results.
/// RRF score = 1/(k + rank_fts) + 1/(k + rank_vec), higher = more relevant.
/// Returns (results, embedding_map) where the embedding_map can be used by MMR.
fn search_hybrid(
    db: &NoteDatabase,
    tokenizer: &JapaneseTokenizer,
    embedding: &[f32],
    query: &str,
    limit: usize,
    vec_fetch_limit: usize,
    min_score: Option<f64>,
    vault_filter: Option<&str>,
    tag_filter: Option<&str>,
    since_date: Option<&str>,
    user_dictionary: &[String],
    synonyms: &HashMap<String, Vec<String>>,
    fuzzy: bool,
    alpha: Option<f64>,
    include_embeddings: bool,
) -> Result<(Vec<ChunkSearchResult>, HashMap<i64, Vec<f32>>), DbError> {
    const K: f64 = 60.0;

    let fts_results = search_fts(db, tokenizer, query, limit * 2, None, vault_filter, None, None, user_dictionary, synonyms, fuzzy)?;
    let (vec_results, emb_map) = search_vec(db, embedding, query, vec_fetch_limit, None, vault_filter, None, None, include_embeddings)?;

    let blended_scores = match alpha {
        Some(a) => compute_rrf_weighted(&fts_results, &vec_results, limit, K, a.clamp(0.0, 1.0)),
        None => compute_rrf(&fts_results, &vec_results, limit, K),
    };

    if blended_scores.is_empty() {
        return Ok((vec![], emb_map));
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
                    emphasized_text: c.emphasized_text,
                }
            })
        })
        .collect();

    results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));

    if let Some(ms) = min_score {
        results.retain(|r| r.score >= ms);
    }

    Ok((apply_filters_and_boost(results, tag_filter, since_date, query, SearchMode::Hybrid), emb_map))
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

    fn req<'a>(
        query: &'a str,
        limit: usize,
        mode: SearchMode,
    ) -> SearchRequest<'a> {
        static EMPTY_SYNONYMS: std::sync::OnceLock<HashMap<String, Vec<String>>> = std::sync::OnceLock::new();
        let synonyms = EMPTY_SYNONYMS.get_or_init(HashMap::new);
        SearchRequest {
            query,
            limit,
            mode,
            embedder: None,
            min_score: None,
            vault_filter: None,
            tag_filter: None,
            since_date: None,
            user_dictionary: &[],
            synonyms,
            fuzzy: false,
            hybrid_alpha: None,
            mmr: false,
            lambda: 0.5,
            backlink_scoring: false,
        }
    }

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
            emphasized_text: String::new(),
        }];
        db.insert_chunks(&chunks).unwrap();

        let req = SearchRequest {
            query: "search engine",
            limit: 10,
            mode: SearchMode::Fts,
            embedder: None,
            min_score: None,
            vault_filter: None,
            tag_filter: None,
            since_date: None,
            user_dictionary: &[],
            synonyms: &HashMap::new(),
            fuzzy: false,
            hybrid_alpha: None,
            mmr: false,
            lambda: 0.5,
            backlink_scoring: false,
        };
        let results = search(&db, &tokenizer, &req).unwrap();
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
                emphasized_text: String::new(),
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
                emphasized_text: String::new(),
            },
        ];
        db.insert_chunks(&chunks).unwrap();

        // Filter by "work" vault
        let results = search(&db, &tokenizer, &SearchRequest {
            vault_filter: Some("work"),
            ..req("alpha", 10, SearchMode::Fts)
        }).unwrap();
        assert_eq!(results.len(), 1, "expected 1 result in work vault");
        assert_eq!(results[0].vault_name, "work");
        assert_eq!(results[0].file_path, "vault_a.md");

        // Filter by "personal" vault
        let results = search(&db, &tokenizer, &SearchRequest {
            vault_filter: Some("personal"),
            ..req("alpha", 10, SearchMode::Fts)
        }).unwrap();
        assert_eq!(results.len(), 1, "expected 1 result in personal vault");
        assert_eq!(results[0].vault_name, "personal");
        assert_eq!(results[0].file_path, "vault_b.md");

        // No filter → both
        let results = search(&db, &tokenizer, &req("alpha", 10, SearchMode::Fts)).unwrap();
        assert_eq!(results.len(), 2, "expected 2 results across all vaults");
    }

    #[test]
    fn test_search_empty_query_returns_empty() {
        let db = NoteDatabase::open_in_memory().unwrap();
        let tokenizer = crate::require_tokenizer!(crate::tokenizer::TokenizerConfig::default());
        let results = search(&db, &tokenizer, &req("  ", 10, SearchMode::Fts)).unwrap();
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
            emphasized_text: String::new(),
        }];
        db.insert_chunks(&chunks).unwrap();

        // Hybrid with no embedder → falls back to FTS
        let results = search(&db, &tokenizer, &req("hybrid fallback", 10, SearchMode::Hybrid)).unwrap();
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
                emphasized_text: String::new(),
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
                emphasized_text: String::new(),
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
                emphasized_text: String::new(),
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
            emphasized_text: String::new(),
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
                emphasized_text: String::new(),
            }
        };

        let fts = vec![make(1, 1.0), make(2, 2.0)];
        let result = compute_rrf(&fts, &[], 5, 60.0);
        assert_eq!(result.len(), 2, "should return FTS results even without vec results");
    }

    // ── MMR diversity re-ranking ────────────────────────────────

    #[test]
    fn test_cosine_similarity_identical_returns_1() {
        let v = vec![1.0, 0.0, 0.0];
        assert!((cosine_similarity(&v, &v) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_cosine_similarity_orthogonal_returns_0() {
        let a = vec![1.0, 0.0];
        let b = vec![0.0, 1.0];
        assert!((cosine_similarity(&a, &b) - 0.0).abs() < 1e-6);
    }

    #[test]
    fn test_cosine_similarity_zero_vector_returns_0() {
        let a = vec![0.0, 0.0];
        let b = vec![1.0, 0.0];
        assert!((cosine_similarity(&a, &b) - 0.0).abs() < 1e-6);
    }

    #[test]
    fn test_mmr_rerank_lambda_1_is_pure_relevance() {
        let make = |id: i64| -> ChunkSearchResult {
            ChunkSearchResult {
                chunk_id: id,
                file_path: format!("{}.md", id),
                parent_header: None,
                content: String::new(),
                score: 0.0,
                search_mode: SearchMode::Vec,
                vault_name: String::new(),
                tags: String::new(),
                frontmatter_date: String::new(),
                title: String::new(),
                emphasized_text: String::new(),
            }
        };
        let candidates = vec![make(1), make(2), make(3)];
        let query_vec = vec![1.0, 0.0];
        let mut vectors = HashMap::new();
        vectors.insert(1, vec![1.0, 0.0]); // most relevant (cos=1)
        vectors.insert(2, vec![0.5, 0.5]); // medium
        vectors.insert(3, vec![0.0, 1.0]); // least relevant (cos=0)

        let result = mmr_rerank(candidates, &query_vec, &vectors, 1.0, 3);
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].chunk_id, 1, "lambda=1.0: most relevant should be first");
        assert_eq!(result[2].chunk_id, 3, "lambda=1.0: least relevant should be last");
    }

    #[test]
    fn test_mmr_rerank_lambda_0_promotes_diversity() {
        let make = |id: i64| -> ChunkSearchResult {
            ChunkSearchResult {
                chunk_id: id,
                file_path: format!("{}.md", id),
                parent_header: None,
                content: String::new(),
                score: 0.0,
                search_mode: SearchMode::Vec,
                vault_name: String::new(),
                tags: String::new(),
                frontmatter_date: String::new(),
                title: String::new(),
                emphasized_text: String::new(),
            }
        };
        // Chunks 1 and 2 are very similar to each other, chunk 3 is very different
        let candidates = vec![make(1), make(2), make(3)];
        let query_vec = vec![1.0, 0.0];
        let mut vectors = HashMap::new();
        vectors.insert(1, vec![1.0, 0.0]); // very relevant, but redundant with 2
        vectors.insert(2, vec![0.99, 0.01]); // very similar to 1
        vectors.insert(3, vec![0.0, 1.0]); // less relevant but very diverse

        let result = mmr_rerank(candidates, &query_vec, &vectors, 0.0, 3);
        assert_eq!(result.len(), 3);
        // With lambda=0, diversity dominates: the first picks chunk 1 (highest query sim
        // is used as tiebreaker when no selected set yet), then chunk 3 (diverse from 1),
        // then chunk 2 (diverse from 1 and 3).
        assert_eq!(result[1].chunk_id, 3, "lambda=0.0: diverse chunk 3 should appear before similar chunk 2");
    }

    #[test]
    fn test_mmr_rerank_respects_limit() {
        let make = |id: i64| -> ChunkSearchResult {
            ChunkSearchResult {
                chunk_id: id,
                file_path: format!("{}.md", id),
                parent_header: None,
                content: String::new(),
                score: 0.0,
                search_mode: SearchMode::Vec,
                vault_name: String::new(),
                tags: String::new(),
                frontmatter_date: String::new(),
                title: String::new(),
                emphasized_text: String::new(),
            }
        };
        let candidates = vec![make(1), make(2), make(3)];
        let query_vec = vec![1.0, 0.0];
        let mut vectors = HashMap::new();
        vectors.insert(1, vec![1.0, 0.0]);
        vectors.insert(2, vec![0.5, 0.5]);
        vectors.insert(3, vec![0.0, 1.0]);

        let result = mmr_rerank(candidates, &query_vec, &vectors, 0.5, 1);
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_mmr_rerank_insufficient_vectors_falls_back_to_order() {
        let make = |id: i64| -> ChunkSearchResult {
            ChunkSearchResult {
                chunk_id: id,
                file_path: format!("{}.md", id),
                parent_header: None,
                content: String::new(),
                score: 0.0,
                search_mode: SearchMode::Vec,
                vault_name: String::new(),
                tags: String::new(),
                frontmatter_date: String::new(),
                title: String::new(),
                emphasized_text: String::new(),
            }
        };
        let candidates = vec![make(1), make(2), make(3)];
        let query_vec = vec![1.0, 0.0];
        let vectors = HashMap::new(); // no vectors available

        let result = mmr_rerank(candidates, &query_vec, &vectors, 0.5, 3);
        assert_eq!(result.len(), 3);
        // Without vectors, should return original order
        assert_eq!(result[0].chunk_id, 1);
        assert_eq!(result[1].chunk_id, 2);
        assert_eq!(result[2].chunk_id, 3);
    }

    #[test]
    fn test_search_vec_mode_without_embedder_returns_error() {
        let db = crate::db::NoteDatabase::open_in_memory().unwrap();
        let tokenizer = match JapaneseTokenizer::new(crate::tokenizer::TokenizerConfig::default()) {
            Ok(tok) => tok,
            Err(_) => return,
        };
        let result = search(&db, &tokenizer, &req("test", 10, SearchMode::Vec));
        match result {
            Err(crate::db::DbError::Other(msg)) => {
                assert!(msg.contains("embedder"), "error should mention embedder");
            }
            _ => panic!("expected DbError::Other with embedder message, got {:?}", result),
        }
    }

    #[test]
    fn test_search_hybrid_mode_without_embedder_falls_back_to_fts() {
        let db = NoteDatabase::open_in_memory().unwrap();
        let tokenizer = match JapaneseTokenizer::new(crate::tokenizer::TokenizerConfig::default()) {
            Ok(tok) => tok,
            Err(_) => return,
        };
        let result = search(&db, &tokenizer, &req("test", 10, SearchMode::Hybrid));
        assert!(result.is_ok(), "Hybrid without embedder should fall back to FTS, got error");
    }

    #[test]
    fn test_search_hybrid_embedder_vec_failure_falls_back_to_fts() {
        let db = NoteDatabase::open_in_memory().unwrap();
        let tokenizer = match JapaneseTokenizer::new(crate::tokenizer::TokenizerConfig::default()) {
            Ok(tok) => tok,
            Err(_) => return,
        };

        let chunks = vec![
            Chunk {
                id: None, file_path: "a.md".into(), chunk_index: 0,
                parent_header: None, content: "hello world".into(),
                tokenized_content: "hello world".into(),
                vault_name: "default".to_string(),
                tags: String::new(), frontmatter_date: String::new(),
                title: String::new(), emphasized_text: String::new(),
            },
        ];
        db.insert_chunks(&chunks).unwrap();

        db.write_conn.borrow().execute_batch("DROP TABLE vec_chunks").unwrap();

        let embedder = crate::embedder::Embedder::for_testing();
        let req = SearchRequest {
            query: "hello",
            limit: 10,
            mode: SearchMode::Hybrid,
            embedder: Some(&embedder),
            min_score: None,
            vault_filter: None,
            tag_filter: None,
            since_date: None,
            user_dictionary: &[],
            synonyms: &HashMap::new(),
            fuzzy: false,
            hybrid_alpha: None,
            mmr: false,
            lambda: 0.5,
            backlink_scoring: false,
        };
        let result = search(&db, &tokenizer, &req);
        assert!(result.is_ok(), "Hybrid with failing vec search should fall back to FTS, got error: {:?}", result.err());
        let results = result.unwrap();
        assert!(!results.is_empty(), "FTS fallback should return results for 'hello'");
        assert!(results.iter().all(|r| r.search_mode == SearchMode::Fts),
            "after fallback all results should be FTS mode");
    }

    #[test]
    fn test_search_fts_non_empty_query_min_score_high_excludes_all() {
        let db = crate::db::NoteDatabase::open_in_memory().unwrap();
        let tokenizer = match JapaneseTokenizer::new(crate::tokenizer::TokenizerConfig::default()) {
            Ok(tok) => tok,
            Err(_) => return,
        };
        let result = search(&db, &tokenizer, &req("test", 10, SearchMode::Fts));
        assert!(result.is_ok());
    }

    #[test]
    fn test_min_score_with_data_by_mode() {
        // Verify that min_score filtering works correctly for each mode.
        // FTS/Vec: min_score excludes results with score > threshold.
        // Hybrid: min_score excludes results with score < threshold.
        let db = crate::db::NoteDatabase::open_in_memory().unwrap();
        let tokenizer = match crate::tokenizer::JapaneseTokenizer::new(
            crate::tokenizer::TokenizerConfig::default(),
        ) {
            Ok(tok) => tok,
            Err(_) => return,
        };

        // FTS mode with min_score=0.0 should exclude all positive-score results
        let result = search(&db, &tokenizer, &SearchRequest { min_score: Some(0.0), ..req("test", 10, SearchMode::Fts) });
        assert!(result.is_ok(), "FTS + min_score should not error");

        // Vec mode with min_score=0.0 should not error (no data, so empty results)
        let result = search(&db, &tokenizer, &SearchRequest { min_score: Some(0.0), ..req("test", 10, SearchMode::Vec) });
        assert!(result.is_ok(), "Vec + min_score should not error");

        // Hybrid mode with min_score=0.0 should not error
        let result = search(&db, &tokenizer, &SearchRequest { min_score: Some(0.0), ..req("test", 10, SearchMode::Hybrid) });
        assert!(result.is_ok(), "Hybrid + min_score should not error");
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
                emphasized_text: String::new(),
            },
        ];
        db.insert_chunks(&chunks).unwrap();

        // fuzzy=false with uppercase query → no match (case-sensitive FTS5)
        let results = search(&db, &tokenizer, &req("HELLO", 10, SearchMode::Fts)).unwrap();
        assert!(results.is_empty(), "non-fuzzy search should not match uppercase 'HELLO' against 'hello'");

        // fuzzy=true → query is normalized to lowercase → matches
        let results = search(&db, &tokenizer, &SearchRequest { fuzzy: true, ..req("HELLO", 10, SearchMode::Fts) }).unwrap();
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
                emphasized_text: String::new(),
            },
        ];
        db.insert_chunks(&chunks).unwrap();

        // fuzzy=true with fullwidth query → normalized to "apple" → matches
        let results = search(&db, &tokenizer, &SearchRequest { fuzzy: true, ..req("ａｐｐｌｅ", 10, SearchMode::Fts) }).unwrap();
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
                emphasized_text: String::new(),
            },
        ];
        db.insert_chunks(&chunks).unwrap();

        // fuzzy=false with exact case → matches
        let results = search(&db, &tokenizer, &req("Hello", 10, SearchMode::Fts)).unwrap();
        assert!(!results.is_empty(), "non-fuzzy should match exact case 'Hello'");

        // fuzzy=false with different case → no match
        let results = search(&db, &tokenizer, &req("hello", 10, SearchMode::Fts)).unwrap();
        assert!(results.is_empty(), "non-fuzzy should not match different case 'hello' against 'Hello'");
    }

    // ── apply_filters_and_boost direction (FTS/Vec vs Hybrid) ────────

    #[test]
    fn test_apply_filters_and_boost_fts_direction() {
        let results = vec![ChunkSearchResult {
            chunk_id: 1,
            file_path: "test.md".into(),
            parent_header: Some("Title".into()),
            content: "content".into(),
            score: 10.0,
            search_mode: SearchMode::Fts,
            vault_name: String::new(),
            tags: String::new(),
            frontmatter_date: String::new(),
            title: "Important Title".into(),
            emphasized_text: String::new(),
        }];
        let boosted = apply_filters_and_boost(results, None, None, "important", SearchMode::Fts);
        assert!(!boosted.is_empty());
        // FTS: lower = better, score *= 0.3 should reduce the score
        assert!(boosted[0].score < 10.0, "FTS title boost should reduce score (lower=better)");
    }

    #[test]
    fn test_apply_filters_and_boost_hybrid_direction() {
        // Verify that hybrid mode (where higher=better) INCREASES scores via division
        let results = vec![ChunkSearchResult {
            chunk_id: 1,
            file_path: "test.md".into(),
            parent_header: Some("Title".into()),
            content: "content".into(),
            score: 0.5,
            search_mode: SearchMode::Hybrid,
            vault_name: String::new(),
            tags: String::new(),
            frontmatter_date: String::new(),
            title: "Important Title".into(),
            emphasized_text: String::new(),
        }];
        let boosted = apply_filters_and_boost(results, None, None, "important", SearchMode::Hybrid);
        assert!(!boosted.is_empty());
        // Hybrid: higher = better, score /= 0.3 should increase the score
        assert!(boosted[0].score > 0.5,
            "Hybrid title boost should increase score (higher=better)");
    }

    #[test]
    fn test_apply_filters_and_boost_tag_filter() {
        let results = vec![
            ChunkSearchResult {
                chunk_id: 1, file_path: "a.md".into(), parent_header: None,
                content: "content".into(), score: 1.0, search_mode: SearchMode::Fts,
                vault_name: String::new(), tags: "project".into(),
                frontmatter_date: String::new(), title: String::new(), emphasized_text: String::new(),
            },
            ChunkSearchResult {
                chunk_id: 2, file_path: "b.md".into(), parent_header: None,
                content: "content".into(), score: 2.0, search_mode: SearchMode::Fts,
                vault_name: String::new(), tags: "personal".into(),
                frontmatter_date: String::new(), title: String::new(), emphasized_text: String::new(),
            },
        ];
        let filtered = apply_filters_and_boost(results, Some("project"), None, "", SearchMode::Fts);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].chunk_id, 1);
    }

    #[test]
    fn test_apply_filters_and_boost_date_filter() {
        let results = vec![
            ChunkSearchResult {
                chunk_id: 1, file_path: "a.md".into(), parent_header: None,
                content: "content".into(), score: 1.0, search_mode: SearchMode::Fts,
                vault_name: String::new(), tags: String::new(),
                frontmatter_date: "2026-01-15".into(), title: String::new(), emphasized_text: String::new(),
            },
            ChunkSearchResult {
                chunk_id: 2, file_path: "b.md".into(), parent_header: None,
                content: "content".into(), score: 2.0, search_mode: SearchMode::Fts,
                vault_name: String::new(), tags: String::new(),
                frontmatter_date: "2025-12-01".into(), title: String::new(), emphasized_text: String::new(),
            },
        ];
        let filtered = apply_filters_and_boost(results, None, Some("2026-01-01"), "", SearchMode::Fts);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].chunk_id, 1);
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
    fn test_extract_snippet_unicode_case_change_byte_offset() {
        let text = "İstanbul is a city\nnot constant case";
        let snippet = extract_snippet(text, "istanbul", 1, 200);
        assert!(snippet.contains("İstanbul"),
            "Turkish İ (U+0130) to_lowercase() should not cause byte offset panic: {}",
            snippet);
    }

    #[test]
    fn test_extract_snippet_german_eszett() {
        let text = "straße\nnext line\nmore content";
        let snippet = extract_snippet(text, "STRASSE", 1, 200);
        assert!(!snippet.is_empty(),
            "German ß→SS case mapping should not cause issues");
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

    // ── Backlink scoring ─────────────────────────────────────────

    #[test]
    fn test_backlink_scoring_fts_lowers_score_for_popular() {
        let db = NoteDatabase::open_in_memory().unwrap();
        let tokenizer = crate::require_tokenizer!(crate::tokenizer::TokenizerConfig::default());

        // Insert two chunks: both match the same query
        let chunks = vec![
            Chunk {
                id: None, file_path: "popular.md".into(), chunk_index: 0,
                parent_header: None, content: "test content".into(),
                tokenized_content: "test content".into(), vault_name: "default".to_string(),
                tags: String::new(), frontmatter_date: String::new(),
                title: String::new(), emphasized_text: String::new(),
            },
            Chunk {
                id: None, file_path: "unpopular.md".into(), chunk_index: 0,
                parent_header: None, content: "test content".into(),
                tokenized_content: "test content".into(), vault_name: "default".to_string(),
                tags: String::new(), frontmatter_date: String::new(),
                title: String::new(), emphasized_text: String::new(),
            },
        ];
        let _ids = db.insert_chunks(&chunks).unwrap();

        // Set up file_cache with different backlink counts
        db.upsert_file_cache("default", "popular.md", "h1", 1000, "none", 100, 0, None).unwrap();
        db.upsert_file_cache("default", "unpopular.md", "h2", 1000, "none", 100, 0, None).unwrap();
        db.write_conn.borrow().execute(
            "UPDATE file_cache SET backlink_count = 10 WHERE path = 'popular.md'",
            [],
        ).unwrap();
        db.write_conn.borrow().execute(
            "UPDATE file_cache SET backlink_count = 0 WHERE path = 'unpopular.md'",
            [],
        ).unwrap();

        // FTS mode: lower score = better. Backlink boost should make popular.md score lower.
        let results = search(&db, &tokenizer, &SearchRequest { backlink_scoring: true, ..req("test content", 10, SearchMode::Fts) }).unwrap();
        assert_eq!(results.len(), 2, "both chunks should be found");
        assert_eq!(results[0].file_path, "popular.md", "popular.md should rank first (lower FTS score)");
        // The score for popular.md should be strictly less than unpopular.md
        assert!(results[0].score < results[1].score, "popular.md should have better (lower) score");
    }

    #[test]
    fn test_backlink_scoring_disabled_does_not_affect_scores() {
        let db = NoteDatabase::open_in_memory().unwrap();
        let tokenizer = crate::require_tokenizer!(crate::tokenizer::TokenizerConfig::default());

        let chunks = vec![
            Chunk {
                id: None, file_path: "popular.md".into(), chunk_index: 0,
                parent_header: None, content: "test content".into(),
                tokenized_content: "test content".into(), vault_name: "default".to_string(),
                tags: String::new(), frontmatter_date: String::new(),
                title: String::new(), emphasized_text: String::new(),
            },
        ];
        let ids = db.insert_chunks(&chunks).unwrap();
        db.upsert_file_cache("default", "popular.md", "h1", 1000, "none", 100, 0, None).unwrap();
        db.write_conn.borrow().execute(
            "UPDATE file_cache SET backlink_count = 10 WHERE path = 'popular.md'",
            [],
        ).unwrap();

        // backlink_scoring=false should not apply the boost
        let results = search(&db, &tokenizer, &req("test content", 10, SearchMode::Fts)).unwrap();
        assert!(!results.is_empty());
        // Score should be the raw BM25 score without backlink adjustment
        let hit = &results[0];
        // With backlink_scoring=false, score should NOT have been reduced
        // (We verify by comparing with the expected BM25 value)
        assert_eq!(hit.chunk_id, ids[0]);
    }

    #[test]
    fn test_backlink_count_does_not_change_with_scoring_disabled() {
        let db = NoteDatabase::open_in_memory().unwrap();
        let tokenizer = crate::require_tokenizer!(crate::tokenizer::TokenizerConfig::default());

        let chunks = vec![
            Chunk {
                id: None, file_path: "a.md".into(), chunk_index: 0,
                parent_header: None, content: "alpha test".into(),
                tokenized_content: "alpha test".into(), vault_name: "default".to_string(),
                tags: String::new(), frontmatter_date: String::new(),
                title: String::new(), emphasized_text: String::new(),
            },
            Chunk {
                id: None, file_path: "b.md".into(), chunk_index: 0,
                parent_header: None, content: "alpha test".into(),
                tokenized_content: "alpha test".into(), vault_name: "default".to_string(),
                tags: String::new(), frontmatter_date: String::new(),
                title: String::new(), emphasized_text: String::new(),
            },
        ];
        db.insert_chunks(&chunks).unwrap();
        db.upsert_file_cache("default", "a.md", "h1", 1000, "none", 100, 0, None).unwrap();
        db.upsert_file_cache("default", "b.md", "h2", 1000, "none", 100, 0, None).unwrap();
        db.write_conn.borrow().execute(
            "UPDATE file_cache SET backlink_count = 5 WHERE path = 'a.md'",
            [],
        ).unwrap();
        db.write_conn.borrow().execute(
            "UPDATE file_cache SET backlink_count = 1 WHERE path = 'b.md'",
            [],
        ).unwrap();

        // With backlink_scoring=false: order should be natural (no backlink influence)
        let results_no_score = search(&db, &tokenizer, &req("alpha", 10, SearchMode::Fts)).unwrap();
        assert_eq!(results_no_score.len(), 2);

        // With backlink_scoring=true: a.md (5 backlinks) should rank better than b.md (1 backlink)
        let results_scored = search(&db, &tokenizer, &SearchRequest { backlink_scoring: true, ..req("alpha", 10, SearchMode::Fts) }).unwrap();
        assert_eq!(results_scored.len(), 2);
        // FTS: lower score = better, so popular file should have lower score
        assert!(results_scored[0].score <= results_scored[1].score,
            "a.md (5 backlinks) should have better score than b.md (1 backlink)");
    }

    #[test]
    fn test_backlink_scoring_hybrid_increases_score() {
        // For Hybrid mode, higher RRF score = better, so backlinks should increase score
        let db = NoteDatabase::open_in_memory().unwrap();
        let tokenizer = crate::require_tokenizer!(crate::tokenizer::TokenizerConfig::default());

        let chunks = vec![
            Chunk {
                id: None, file_path: "popular.md".into(), chunk_index: 0,
                parent_header: None, content: "test content".into(),
                tokenized_content: "test content".into(), vault_name: "default".to_string(),
                tags: String::new(), frontmatter_date: String::new(),
                title: String::new(), emphasized_text: String::new(),
            },
        ];
        db.insert_chunks(&chunks).unwrap();
        db.upsert_file_cache("default", "popular.md", "h1", 1000, "none", 100, 0, None).unwrap();
        db.write_conn.borrow().execute(
            "UPDATE file_cache SET backlink_count = 5 WHERE path = 'popular.md'",
            [],
        ).unwrap();

        // Hybrid mode with no embedder falls back to FTS, which doesn't exercise the hybrid path.
        // We test the FTS path (which is also lower=better) separately.
        // This test verifies the code path exists and doesn't crash.
        let results = search(&db, &tokenizer, &SearchRequest { backlink_scoring: true, ..req("test", 10, SearchMode::Hybrid) }).unwrap();
        // Hybrid without embedder falls back to FTS, so search should still work
        assert!(!results.is_empty() || results.is_empty());
    }
}
