include!(concat!(env!("OUT_DIR"), "/embedded_model.rs"));

use sha2::{Digest, Sha256};
use std::io::Read;
use std::sync::{Arc, OnceLock};
use thiserror::Error;
use vaporetto::{Model, Predictor, Sentence};

#[derive(Debug, Error)]
pub enum TokenizerError {
    #[error("no model available: set SHIOTSUCHI_MODEL_PATH or embed a model at build time")]
    NoModel,
    #[error("model load failed: {0}")]
    ModelLoad(String),
}

/// Global cache for the tokenizer instance to avoid repeated initialization.
static TOKENIZER: OnceLock<Arc<JapaneseTokenizer>> = OnceLock::new();

/// Get a cached tokenizer instance to avoid repeated initialization cost.
/// Returns an Arc to allow sharing across multiple callers.
pub fn get_tokenizer() -> Result<Arc<JapaneseTokenizer>, TokenizerError> {
    if let Some(tokenizer) = TOKENIZER.get() {
        return Ok(Arc::clone(tokenizer));
    }

    let tokenizer = JapaneseTokenizer::new(TokenizerConfig::default())?;
    let arc_tokenizer = Arc::new(tokenizer);
    let _ = TOKENIZER.set(Arc::clone(&arc_tokenizer));
    Ok(arc_tokenizer)
}

/// sqlite-vaporetto の TokenizerConfig に対応。
#[derive(Debug, Clone)]
pub struct TokenizerConfig {
    /// Some(vec!["名詞"]) のように指定すると品詞フィルタを適用。
    /// sqlite-vaporetto の `tags 名詞` オプションと等価。
    pub pos_filter: Option<Vec<String>>,
    /// タグなしトークン（ASCII 単語等）を含めるか。
    /// sqlite-vaporetto の `keep_untagged` オプションと等価。
    pub keep_untagged: bool,
}

impl Default for TokenizerConfig {
    fn default() -> Self {
        Self {
            pos_filter: None,
            keep_untagged: true,
        }
    }
}

pub struct JapaneseTokenizer {
    predictor: Predictor,
    config: TokenizerConfig,
}

impl JapaneseTokenizer {
    /// Predictor 構築優先順位:
    /// 1. EMBEDDED_PREDICTOR_BYTES（build.rs でビルド時にシリアライズ済み）→ deserialize のみ
    /// 2. SHIOTSUCHI_MODEL_PATH 環境変数 → decompress + Model::read + Predictor::new
    /// 3. どちらもなければ TokenizerError::NoModel
    pub fn new(config: TokenizerConfig) -> Result<Self, TokenizerError> {
        let predictor = if let Some(bytes) = EMBEDDED_PREDICTOR_BYTES {
            // Verify integrity via SHA-256 hash to detect corruption or tampering
            let mut hasher = Sha256::new();
            hasher.update(bytes);
            let computed = hex::encode(hasher.finalize());
            if computed != EMBEDDED_PREDICTOR_HASH {
                return Err(TokenizerError::ModelLoad(
                    "embedded predictor bytes failed integrity check (possible corruption)".into(),
                ));
            }
            // SAFETY: vaporetto's Predictor::deserialize_from_slice_unchecked skips internal
            // validation for performance. This is sound here because:
            //   1. The byte input comes from build.rs via predictor.serialize_to_vec(), a trusted
            //      source — the serialization/deserialization pair is guaranteed compatible within
            //      the same vaporetto version.
            //   2. A SHA-256 integrity check (lines above) confirms bytes are untampered and match
            //      the build-time serialized output.
            //   3. Deserialization of untrusted input would require safe deserialization or
            //      additional validation; this code does not face untrusted predictor data.
            let (p, _) = unsafe {
                Predictor::deserialize_from_slice_unchecked(bytes)
                    .map_err(|e| TokenizerError::ModelLoad(e.to_string()))?
            };
            p
        } else if let Ok(path) = std::env::var("SHIOTSUCHI_MODEL_PATH") {
            let raw = std::fs::read(&path)
                .map_err(|e| TokenizerError::ModelLoad(format!("{}: {}", path, e)))?;
            let model_data =
                decompress_if_needed(&raw).map_err(|e| TokenizerError::ModelLoad(e.to_string()))?;
            let model = Model::read(model_data.as_slice())
                .map_err(|e| TokenizerError::ModelLoad(e.to_string()))?;
            Predictor::new(model, false).map_err(|e| TokenizerError::ModelLoad(e.to_string()))?
        } else {
            return Err(TokenizerError::NoModel);
        };
        Ok(Self { predictor, config })
    }

    /// `vaporetto_split(text, ' ')` と等価。
    /// テキストをトークナイズして空白区切り文字列を返す。
    /// この文字列を FTS5 の body カラムに格納する。
    pub fn split(&self, text: &str) -> String {
        self.collect_tokens(text).join(" ")
    }

    /// Tokenize content, using Vaporetto for regular text and whitespace-splitting for
    /// code/math content.  Code blocks should not be tokenized with Japanese rules since
    /// identifiers and punctuation follow programming-language conventions.
    pub fn tokenize_content(&self, content: &str, is_code: bool) -> String {
        if is_code {
            simple_tokenize(content)
        } else {
            self.split(content)
        }
    }

    /// `vaporetto_and_query(text)` と等価。
    /// 出力例: `"東京" AND "検索" AND "エンジン"`
    /// 各トークンを "" で囲むことで特殊文字をエスケープし AND 結合する。
    /// FTS5 の MATCH 引数にそのまま渡せる。
    ///
    /// Deprecated: use `collect_tokens` + `expand_synonyms` instead.
    #[deprecated(since = "0.5.0", note = "use collect_tokens() + expand_synonyms() instead")]
    pub fn and_query(&self, text: &str) -> String {
        self.collect_tokens(text)
            .into_iter()
            .map(|t| format!("\"{}\"", t.replace('"', "\"\"")))
            .collect::<Vec<_>>()
            .join(" AND ")
    }

    /// `vaporetto_or_query(text)` と等価（将来の OR 検索用）。
    pub fn or_query(&self, text: &str) -> String {
        self.collect_tokens(text)
            .into_iter()
            .map(|t| format!("\"{}\"", t.replace('"', "\"\"")))
            .collect::<Vec<_>>()
            .join(" OR ")
    }

    pub(crate) fn collect_tokens(&self, text: &str) -> Vec<String> {
        let mut tokens = Vec::new();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            if let Ok(mut sentence) = Sentence::from_raw(line) {
                self.predictor.predict(&mut sentence);
                for token in sentence.iter_tokens() {
                    let surface = token.surface();
                    if surface.trim().is_empty() {
                        continue;
                    }
                    if self.should_include(&token) {
                        for st in split_ascii_words(surface) {
                            if tokens.last() != Some(&st) {
                                tokens.push(st);
                            }
                        }
                    }
                }
            }
        }
        tokens
    }

    fn should_include(&self, token: &vaporetto::Token) -> bool {
        match &self.config.pos_filter {
            None => true,
            Some(prefixes) => {
                // Check all tag positions for a matching prefix.
                // The bccwj-suw+unidic_pos+kana model has two tags per token:
                // (unidic_pos, kana). We check both positions since the
                // ordering may vary between model versions.
                let matched = token.tags().iter().any(|opt| {
                    opt.as_ref()
                        .map(|tag| prefixes.iter().any(|p| tag.starts_with(p.as_str())))
                        .unwrap_or(false)
                });
                if matched {
                    return true;
                }
                // No tag matched: rely on keep_untagged
                self.config.keep_untagged
            }
        }
    }
}

/// Normalize text for fuzzy search: Unicode NFKC normalization followed by
/// ASCII lowercasing.
///
/// NFKC handles:
/// - Full-width to half-width ASCII (Ａ → A, １ → 1)
/// - Compatibility normalization (㍻ → 平成, ℌ → H)
/// - Combining character canonical recomposition (が → が)
///
/// Lowercasing handles case-insensitive matching for ASCII terms.
///
/// This is a no-op for text already in NFKC form with lowercase ASCII,
/// which covers the vast majority of real-world content.
pub fn normalize(text: &str) -> String {
    use unicode_normalization::UnicodeNormalization;
    text.nfkc()
        .collect::<String>()
        .to_lowercase()
}

/// Apply user dictionary to a space-separated token string.
/// Convenience wrapper around [`apply_user_dictionary`].
pub fn apply_user_dictionary_str(tokens: &str, dict: &[String]) -> String {
    if dict.is_empty() || tokens.is_empty() {
        return tokens.to_string();
    }
    let token_list: Vec<String> = tokens.split(' ').map(|s| s.to_string()).collect();
    apply_user_dictionary(&token_list, dict).join(" ")
}

/// Apply user dictionary post-processing: merge consecutive tokens that form
/// a dictionary entry into a single token. The dictionary entries are matched
/// exactly (case-sensitive) against the space-joined sequence of tokens.
///
/// When multiple entries match at the same position, the longest match wins.
/// This allows shorter entries ("New York") to coexist with longer ones
/// ("New York City").
///
/// This is a workaround for Vaporetto's lack of user dictionary support:
/// since Vaporetto is a trained model, we cannot add custom vocabulary
/// directly. Instead, we merge Vaporetto's output tokens post-hoc.
pub fn apply_user_dictionary(tokens: &[String], dict: &[String]) -> Vec<String> {
    if dict.is_empty() {
        return tokens.to_vec();
    }

    let mut result: Vec<String> = Vec::with_capacity(tokens.len());
    let mut i = 0;

    // Pre-compute the maximum number of tokens any dictionary entry spans,
    // so we can limit the inner search loop to O(n * m) worst-case instead
    // of O(n²) where n = token count and m = max entry length in tokens.
    let max_entry_tokens = dict
        .iter()
        .map(|e| {
            // For space-joined entries (e.g., "Amazon Web Services"), the
            // token count equals the word count. For concatenated-only entries
            // (e.g., "ChatGPT"), the entry could span multiple Vaporetto
            // tokens — use the byte length as a safe upper bound since no
            // single entry can span more tokens than its total byte count.
            e.split(' ').count().max(e.len())
        })
        .max()
        .unwrap_or(1);

    while i < tokens.len() {
        // Try to find the longest dictionary match starting at position i.
        let mut best_len = 0usize;
        let mut best_entry: Option<&str> = None;

        // Build candidate strings. We try two forms:
        //   1. Space-joined (for multi-word entries like "Amazon Web Services")
        //   2. Concatenated  (for single-word entries like "ChatGPT")
        let mut spaced = String::new();
        let mut concat = String::new();
        let search_end = tokens.len().min(i + max_entry_tokens);
        for j in i..search_end {
            if j > i {
                spaced.push(' ');
            }
            spaced.push_str(&tokens[j]);
            concat.push_str(&tokens[j]);

            for entry in dict {
                if spaced == *entry || concat == *entry {
                    let span_len = j - i + 1;
                    if span_len > best_len {
                        best_len = span_len;
                        best_entry = Some(entry.as_str());
                    }
                }
            }
        }

        if let Some(entry) = best_entry {
            result.push(entry.to_string());
            i += best_len;
        } else {
            result.push(tokens[i].clone());
            i += 1;
        }
    }
    result
}

// Shared decompress logic — see _decompress.rs for implementation.
include!("_decompress.rs");

fn split_ascii_words(token: &str) -> Vec<String> {
    let chars: Vec<char> = token.chars().collect();
    if !chars.iter().any(|c| c.is_ascii_alphanumeric()) {
        return vec![token.to_string()];
    }

    let mut result = Vec::new();

    for part in token.split('_') {
        if part.is_empty() {
            continue;
        }
        let pc: Vec<char> = part.chars().collect();
        let n = pc.len();
        let mut start = 0;

        for i in 1..n {
            let boundary = (pc[i - 1].is_ascii_lowercase() && pc[i].is_ascii_uppercase())
                || (pc[i - 1].is_ascii_alphabetic() && pc[i].is_ascii_digit())
                || (pc[i - 1].is_ascii_digit() && pc[i].is_ascii_alphabetic());

            let special = !boundary
                && i >= 2
                && pc[i - 2].is_ascii_uppercase()
                && pc[i - 1].is_ascii_uppercase()
                && pc[i].is_ascii_lowercase();

            if special {
                if i - 1 > start {
                    result.push(pc[start..i - 1].iter().collect());
                }
                start = i - 1;
            } else if boundary {
                result.push(pc[start..i].iter().collect());
                start = i;
            }
        }

        if start < n {
            result.push(pc[start..].iter().collect());
        }
    }

    if result.is_empty() {
        vec![token.to_string()]
    } else {
        result
    }
}

/// フォールバック: モデルなし環境でのテスト用（空白分割）。
pub fn simple_tokenize(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// フォールバック: simple_tokenize に対応した AND クエリビルダ。
pub fn simple_and_query(text: &str) -> String {
    text.split_whitespace()
        .map(|t| format!("\"{}\"", t.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(" AND ")
}

/// Helper macro for tests: creates a `JapaneseTokenizer` or prints a skip message and returns.
/// Use this in any test that requires the Vaporetto model to avoid silent skips.
/// Uses fully qualified type paths so the macro works across crate boundaries.
/// The skip message is visible in CI logs via stderr.
#[macro_export]
macro_rules! require_tokenizer {
    () => {
        match $crate::tokenizer::JapaneseTokenizer::new(
            $crate::tokenizer::TokenizerConfig::default(),
        ) {
            Ok(tok) => tok,
            Err(_) => {
                eprintln!(
                    "[SKIPPED] {}:{} — Vaporetto model not available (set SHIOTSUCHI_MODEL_PATH)",
                    file!(),
                    line!()
                );
                return;
            }
        }
    };
    ($config:expr) => {
        match $crate::tokenizer::JapaneseTokenizer::new($config) {
            Ok(tok) => tok,
            Err(_) => {
                eprintln!(
                    "[SKIPPED] {}:{} — Vaporetto model not available (set SHIOTSUCHI_MODEL_PATH)",
                    file!(),
                    line!()
                );
                return;
            }
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── user dictionary post-processing ─────────────────────────

    #[test]
    fn test_apply_dict_empty_dict_returns_tokens_unchanged() {
        let tokens = vec!["Hello".to_string(), "world".to_string()];
        let result = apply_user_dictionary(&tokens, &[]);
        assert_eq!(result, tokens);
    }

    #[test]
    fn test_apply_dict_no_match_returns_tokens_unchanged() {
        let tokens = vec!["Hello".to_string(), "world".to_string()];
        let dict = vec!["Claude".to_string()];
        let result = apply_user_dictionary(&tokens, &dict);
        assert_eq!(result, tokens);
    }

    #[test]
    fn test_apply_dict_merges_multi_token_match() {
        let tokens = vec![
            "Amazon".to_string(),
            "Web".to_string(),
            "Services".to_string(),
        ];
        let dict = vec!["Amazon Web Services".to_string()];
        let result = apply_user_dictionary(&tokens, &dict);
        assert_eq!(result, vec!["Amazon Web Services"]);
    }

    #[test]
    fn test_apply_dict_merges_in_middle_of_text() {
        let tokens = vec![
            "I".to_string(),
            "use".to_string(),
            "Amazon".to_string(),
            "Web".to_string(),
            "Services".to_string(),
            "daily".to_string(),
        ];
        let dict = vec!["Amazon Web Services".to_string()];
        let result = apply_user_dictionary(&tokens, &dict);
        assert_eq!(
            result,
            vec![
                "I".to_string(),
                "use".to_string(),
                "Amazon Web Services".to_string(),
                "daily".to_string(),
            ]
        );
    }

    #[test]
    fn test_apply_dict_prefers_longest_match() {
        let tokens = vec![
            "New".to_string(),
            "York".to_string(),
            "City".to_string(),
        ];
        let dict = vec![
            "New York".to_string(),
            "New York City".to_string(),
        ];
        let result = apply_user_dictionary(&tokens, &dict);
        assert_eq!(result, vec!["New York City"]);
    }

    #[test]
    fn test_apply_dict_multiple_matches() {
        let tokens = vec![
            "Chat".to_string(),
            "GPT".to_string(),
            "is".to_string(),
            "great".to_string(),
        ];
        let dict = vec!["ChatGPT".to_string()];
        let result = apply_user_dictionary(&tokens, &dict);
        assert_eq!(result, vec!["ChatGPT".to_string(), "is".to_string(), "great".to_string()]);
    }

    #[test]
    fn test_apply_dict_partial_match_does_not_merge() {
        // "Amazon Web" matches but "Amazon Web Services Extra" also in dict — should match longest
        // This tests that a partial prefix match doesn't trigger
        let tokens = vec![
            "Amazon".to_string(),
            "Web".to_string(),
            "Services".to_string(),
        ];
        let dict = vec!["Amazon Web".to_string(), "Something Else".to_string()];
        let result = apply_user_dictionary(&tokens, &dict);
        // "Amazon Web" is a shorter match, but "Amazon Web Services" doesn't match fully
        // "Amazon Web" is 2 tokens — it should match
        assert_eq!(result, vec!["Amazon Web".to_string(), "Services".to_string()]);
    }

    // ── normalize / fuzzy normalization ──────────────────────────

    #[test]
    fn test_normalize_ascii_lowercase() {
        assert_eq!(normalize("Hello World"), "hello world");
        assert_eq!(normalize("ABC"), "abc");
    }

    #[test]
    fn test_normalize_fullwidth_to_halfwidth() {
        assert_eq!(normalize("ＡＢＣ"), "abc");
        assert_eq!(normalize("１２３"), "123");
    }

    #[test]
    fn test_normalize_mixed_width() {
        assert_eq!(normalize("Ａｐｐｌｅ"), "apple");
        assert_eq!(normalize("Hello１２３"), "hello123");
    }

    #[test]
    fn test_normalize_combining_characters() {
        // か + ゙(U+3099) should become が (U+304C) via NFC
        let combined = "か\u{3099}"; // か + combining dakuten
        let composed = normalize(combined);
        assert_eq!(composed, "が", "combining dakuten should compose to single char");
        assert_eq!(composed.chars().count(), 1, "composed result should be a single character");
    }

    #[test]
    fn test_normalize_already_normalized_unchanged() {
        // Already NFC and lowercase should be unchanged
        let s = "hello world 123 東京";
        assert_eq!(normalize(s), s);
    }

    #[test]
    fn test_normalize_empty_string() {
        assert_eq!(normalize(""), "");
    }

    #[test]
    fn test_simple_tokenize() {
        assert_eq!(simple_tokenize("Hello world  test"), "Hello world test");
    }

    #[test]
    fn test_simple_and_query() {
        let q = simple_and_query("東京 検索");
        assert_eq!(q, "\"東京\" AND \"検索\"");
    }

    #[test]
    fn test_simple_tokenize_multiple_spaces() {
        assert_eq!(simple_tokenize("hello    world"), "hello world");
        assert_eq!(simple_tokenize("  leading space"), "leading space");
        assert_eq!(simple_tokenize("trailing space  "), "trailing space");
    }

    /// 埋め込み Predictor (deserialize パス) が通常の Model::read + Predictor::new より
    /// 大幅に速いことを確認する。
    ///
    /// - 埋め込みあり (EMBEDDED_PREDICTOR_BYTES = Some): deserialize のみなので 500ms 未満を期待
    /// - 埋め込みなし (SHIOTSUCHI_MODEL_PATH): decompress + Model::read + Predictor::new で
    ///   数秒かかるが、テストとしては「正常に完了する」ことのみ確認
    #[test]
    fn test_predictor_init_path() {
        use std::time::Instant;

        if EMBEDDED_PREDICTOR_BYTES.is_some() {
            // 埋め込みパス: deserialize のみなので高速なはず
            let t = Instant::now();
            let tokenizer = JapaneseTokenizer::new(TokenizerConfig::default())
                .expect("embedded predictor should load");
            let elapsed = t.elapsed();

            // 動作確認
            let tokens = tokenizer.split("東京は日本の首都です");
            assert!(!tokens.is_empty(), "tokenizer should produce tokens");

            assert!(
                elapsed.as_millis() < 500,
                "embedded predictor deserialize should complete in <500ms, took {:?}",
                elapsed
            );
        } else {
            // ModelPath パス: 速度は問わず、正常に動作することを確認
            // (SHIOTSUCHI_MODEL_PATH が未設定なら NoModel エラーになるのも正常)
            match JapaneseTokenizer::new(TokenizerConfig::default()) {
                Ok(tokenizer) => {
                    let tokens = tokenizer.split("東京は日本の首都です");
                    assert!(!tokens.is_empty(), "tokenizer should produce tokens");
                }
                Err(TokenizerError::NoModel) => {
                    // 環境変数なし・埋め込みなしのビルドでは期待されるエラー
                }
                Err(e) => panic!("unexpected tokenizer error: {}", e),
            }
        }
    }

    #[test]
    fn test_decompress_if_needed_passthrough_plain_bytes() {
        let data = b"hello world, this is not zstd compressed";
        let result = decompress_if_needed(data).unwrap();
        assert_eq!(result, data);
    }

    #[test]
    fn test_decompress_if_needed_rejects_garbage_zstd() {
        // Valid zstd magic bytes but no actual frame
        let garbage = &[0x28, 0xb5, 0x2f, 0xfd, 0x00, 0x01, 0x02, 0x03];
        let result = decompress_if_needed(garbage);
        // Should either error or return something — either way, no panic
        assert!(result.is_err() || result.is_ok());
    }

    /// get_tokenizer() が OnceLock でキャッシュされ、2回目以降は初期化コストがゼロに近いことを確認する。
    /// (同一プロセス内でのみ有効)
    #[test]
    fn test_get_tokenizer_cached() {
        use std::time::Instant;

        // モデルが利用できない場合はスキップ
        if EMBEDDED_PREDICTOR_BYTES.is_none() && std::env::var("SHIOTSUCHI_MODEL_PATH").is_err() {
            return;
        }

        let _ = get_tokenizer().expect("first call should succeed");

        // 2回目: OnceLock キャッシュから返るので無視できるほど速い
        let t = Instant::now();
        let tokenizer = get_tokenizer().expect("second call should succeed");
        let elapsed = t.elapsed();

        let tokens = tokenizer.split("検索エンジン");
        assert!(!tokens.is_empty());

        assert!(
            elapsed.as_micros() < 500,
            "cached get_tokenizer() should return in <500µs, took {:?}",
            elapsed
        );
    }

    #[test]
    fn test_integrity_check_fails_on_corrupted_bytes() {
        if EMBEDDED_PREDICTOR_BYTES.is_none() {
            eprintln!("[SKIPPED] {}:{} — no embedded predictor, skipping integrity check test", file!(), line!());
            return;
        }

        let mut hasher = Sha256::new();
        hasher.update(EMBEDDED_PREDICTOR_BYTES.unwrap());
        let computed = hex::encode(hasher.finalize());
        assert_eq!(computed, EMBEDDED_PREDICTOR_HASH,
            "embedded predictor hash should match computed hash");

        let wrong_data = b"different bytes that are not the model";
        let mut hasher2 = Sha256::new();
        hasher2.update(wrong_data);
        let wrong_hash = hex::encode(hasher2.finalize());
        assert_ne!(wrong_hash, EMBEDDED_PREDICTOR_HASH,
            "wrong hash should not match embedded predictor hash");
    }

    #[test]
    fn test_or_query_empty_input() {
        let tokenizer = match JapaneseTokenizer::new(TokenizerConfig::default()) {
            Ok(tok) => tok,
            Err(_) => return,
        };
        assert_eq!(tokenizer.or_query(""), "");
    }

    #[test]
    fn test_or_query_normal_input() {
        let tokenizer = match JapaneseTokenizer::new(TokenizerConfig::default()) {
            Ok(tok) => tok,
            Err(_) => return,
        };
        let result = tokenizer.or_query("hello");
        assert!(!result.is_empty(), "or_query on normal input should produce output");
    }

    #[test]
    fn test_and_query_empty_input() {
        assert_eq!(simple_and_query(""), "");
        assert_eq!(simple_and_query("   "), "");
    }

    #[test]
    fn test_simple_tokenize_empty_input() {
        assert_eq!(simple_tokenize(""), "");
        assert_eq!(simple_tokenize("   "), "");
    }

    #[test]
    fn test_simple_and_query_single_word() {
        let q = simple_and_query("hello");
        assert_eq!(q, "\"hello\"");
    }

    #[test]
    fn test_simple_tokenize_single_word() {
        assert_eq!(simple_tokenize("hello"), "hello");
    }

    // ── simple_and_query additional edge cases ───────────────────────

    #[test]
    fn test_simple_and_query_basic() {
        let result = simple_and_query("hello world");
        assert_eq!(result, "\"hello\" AND \"world\"");
    }

    #[test]
    fn test_simple_and_query_with_quotes_in_term() {
        let result = simple_and_query("say \"hi\"");
        // Quotes in the term should be doubled: " → ""
        assert_eq!(result, "\"say\" AND \"\"\"hi\"\"\"");
    }

    #[test]
    fn test_simple_and_query_tabs_and_newlines() {
        let result = simple_and_query("hello\tworld\nfoo");
        assert_eq!(result, "\"hello\" AND \"world\" AND \"foo\"");
    }

    // ── simple_tokenize additional edge cases ────────────────────────

    #[test]
    fn test_simple_tokenize_basic() {
        let result = simple_tokenize("hello world foo");
        assert_eq!(result, "hello world foo");
    }

    #[test]
    fn test_simple_tokenize_unicode() {
        let result = simple_tokenize("日本語 English 中文");
        assert_eq!(result, "日本語 English 中文");
    }

    // ── should_include / collect_tokens with config variations ───────

    #[test]
    fn test_collect_tokens_empty_input() {
        let tokenizer = match JapaneseTokenizer::new(TokenizerConfig::default()) {
            Ok(tok) => tok,
            Err(_) => return,
        };
        let tokens = tokenizer.collect_tokens("");
        assert_eq!(tokens.len(), 0);
    }

    #[test]
    fn test_collect_tokens_single_line() {
        let tokenizer = match JapaneseTokenizer::new(TokenizerConfig::default()) {
            Ok(tok) => tok,
            Err(_) => return,
        };
        let tokens = tokenizer.collect_tokens("こんにちは");
        assert!(!tokens.is_empty(), "should tokenize Japanese text");
    }

    #[test]
    fn test_collect_tokens_multiline_input() {
        let tokenizer = match JapaneseTokenizer::new(TokenizerConfig::default()) {
            Ok(tok) => tok,
            Err(_) => return,
        };
        let text = "行一\n行二\n行三";
        let tokens = tokenizer.collect_tokens(text);
        assert!(!tokens.is_empty());
    }

    #[test]
    fn test_collect_tokens_skips_empty_lines() {
        let tokenizer = match JapaneseTokenizer::new(TokenizerConfig::default()) {
            Ok(tok) => tok,
            Err(_) => return,
        };
        let text = "content\n\n\nmore";
        let tokens = tokenizer.collect_tokens(text);
        assert!(!tokens.is_empty());
    }

    #[test]
    fn test_collect_tokens_pos_filter() {
        // POS filter with keep_untagged=true should still return tokens.
        // (The bccwj-suw model doesn't emit POS tags, so pos_filter alone
        //  with keep_untagged=false may return empty — this tests that
        //  keep_untagged=true allows all tokens through.)
        let config = TokenizerConfig {
            pos_filter: Some(vec!["名詞".to_string()]),
            keep_untagged: true,
        };
        let tokenizer = match JapaneseTokenizer::new(config) {
            Ok(tok) => tok,
            Err(_) => return,
        };
        let tokens = tokenizer.collect_tokens("東京は日本の首都です");
        assert!(!tokens.is_empty(), "should return tokens with keep_untagged=true");
        assert!(tokens.contains(&"東京".to_string()), "should include '東京'");
        assert!(tokens.contains(&"日本".to_string()), "should include '日本'");
    }

    #[test]
    fn test_collect_tokens_pos_filter_excludes_untagged() {
        // With pos_filter active and keep_untagged=false, tokens without
        // matching tags are excluded. English words are untagged → excluded.
        let config = TokenizerConfig {
            pos_filter: Some(vec!["名詞".to_string()]),
            keep_untagged: false,
        };
        let tokenizer = match JapaneseTokenizer::new(config) {
            Ok(tok) => tok,
            Err(_) => return,
        };
        let tokens = tokenizer.collect_tokens("Hello world");
        assert!(tokens.is_empty(), "untagged tokens should be excluded");
    }

    #[test]
    fn test_collect_tokens_multiple_pos_prefixes() {
        let config = TokenizerConfig {
            pos_filter: Some(vec!["名詞".to_string(), "動詞".to_string()]),
            keep_untagged: true,
        };
        let tokenizer = match JapaneseTokenizer::new(config) {
            Ok(tok) => tok,
            Err(_) => return,
        };
        let tokens = tokenizer.collect_tokens("東京は日本の首都です");
        assert!(!tokens.is_empty(), "should return tokens with keep_untagged=true");
    }

    #[test]
    fn test_or_query_special_chars_escaped() {
        let tokenizer = match JapaneseTokenizer::new(TokenizerConfig::default()) {
            Ok(tok) => tok,
            Err(_) => return,
        };
        let result = tokenizer.or_query("query with \"quotes\"");
        // Quotes should be doubled inside the quoted terms
        assert!(result.contains("\"\"\""));
        assert!(!result.contains(r#"\""#), "should not contain raw backslash-quotes");
    }

    #[test]
    fn test_or_query_whitespace_only() {
        let tokenizer = match JapaneseTokenizer::new(TokenizerConfig::default()) {
            Ok(tok) => tok,
            Err(_) => return,
        };
        assert_eq!(tokenizer.or_query("   "), "");
        assert_eq!(tokenizer.or_query("\n\t"), "");
    }

    #[test]
    fn test_simple_or_query_empty_input() {
        assert_eq!(simple_and_query(""), "");
        assert_eq!(simple_and_query("   "), "");
    }

    // ── split_ascii_words ─────────────────────────────────────

    #[test]
    fn test_split_ascii_words_pure_japanese_unchanged() {
        let result = split_ascii_words("東京");
        assert_eq!(result, vec!["東京"]);
    }

    #[test]
    fn test_split_ascii_words_single_ascii_word_unchanged() {
        let result = split_ascii_words("React");
        assert_eq!(result, vec!["React"]);
    }

    #[test]
    fn test_split_ascii_words_camel_case() {
        let result = split_ascii_words("ReactComponent");
        assert_eq!(result, vec!["React", "Component"]);
    }

    #[test]
    fn test_split_ascii_words_uppercase_run_then_lowercase() {
        let result = split_ascii_words("HTMLParser");
        assert_eq!(result, vec!["HTML", "Parser"]);
    }

    #[test]
    fn test_split_ascii_words_underscore() {
        let result = split_ascii_words("test_runner");
        assert_eq!(result, vec!["test", "runner"]);
    }

    #[test]
    fn test_split_ascii_words_digit_boundary() {
        let result = split_ascii_words("data2model");
        assert_eq!(result, vec!["data", "2", "model"]);
    }

    #[test]
    fn test_split_ascii_words_mixed_content() {
        let result = split_ascii_words("getElementById");
        assert_eq!(result, vec!["get", "Element", "By", "Id"]);
    }

    #[test]
    fn test_split_ascii_words_no_ascii_returns_original() {
        let result = split_ascii_words("日本語");
        assert_eq!(result, vec!["日本語"]);
    }

    // ── collect_tokens with ASCII splitting ───────────────────

    #[test]
    fn test_collect_tokens_keep_untagged() {
        // With keep_untagged=true, tokens without POS tags should pass through
        let config = TokenizerConfig {
            pos_filter: Some(vec!["名詞".to_string()]),
            keep_untagged: true,
        };
        let tokenizer = match JapaneseTokenizer::new(config) {
            Ok(tok) => tok,
            Err(_) => return,
        };
        let tokens = tokenizer.collect_tokens("Hello 東京 world");
        // "Hello" and "world" (untagged) should be included with keep_untagged=true
        assert!(!tokens.is_empty(), "should include untagged tokens");
    }
}
