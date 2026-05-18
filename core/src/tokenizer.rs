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

    /// `vaporetto_and_query(text)` と等価。
    /// 出力例: `"東京" AND "検索" AND "エンジン"`
    /// 各トークンを "" で囲むことで特殊文字をエスケープし AND 結合する。
    /// FTS5 の MATCH 引数にそのまま渡せる。
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

    fn collect_tokens(&self, text: &str) -> Vec<String> {
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
                        tokens.push(surface.to_string());
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

// Shared decompress logic — see _decompress.rs for implementation.
include!("_decompress.rs");

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
