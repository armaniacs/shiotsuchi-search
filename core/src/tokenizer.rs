include!(concat!(env!("OUT_DIR"), "/embedded_model.rs"));

use std::io::Read;
use std::sync::{Arc, OnceLock};
use thiserror::Error;
use vaporetto::{Model, Predictor, Sentence};
use sha2::{Sha256, Digest};

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
        Self { pos_filter: None, keep_untagged: true }
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
            let computed = format!("{:x}", hasher.finalize());
            if computed != EMBEDDED_PREDICTOR_HASH {
                return Err(TokenizerError::ModelLoad(
                    "embedded predictor bytes failed integrity check (possible corruption)".into(),
                ));
            }
            // SAFETY: bytes passed hash check and come from build.rs (serialized via predictor.serialize_to_vec())
            let (p, _) = unsafe {
                Predictor::deserialize_from_slice_unchecked(bytes)
                    .map_err(|e| TokenizerError::ModelLoad(e.to_string()))?
            };
            p
        } else if let Ok(path) = std::env::var("SHIOTSUCHI_MODEL_PATH") {
            let raw = std::fs::read(&path)
                .map_err(|e| TokenizerError::ModelLoad(format!("{}: {}", path, e)))?;
            let model_data = decompress_if_needed(&raw)
                .map_err(|e| TokenizerError::ModelLoad(e.to_string()))?;
            let model = Model::read(model_data.as_slice())
                .map_err(|e| TokenizerError::ModelLoad(e.to_string()))?;
            Predictor::new(model, false)
                .map_err(|e| TokenizerError::ModelLoad(e.to_string()))?
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
            if line.is_empty() { continue; }
            if let Ok(mut sentence) = Sentence::from_raw(line) {
                self.predictor.predict(&mut sentence);
                for token in sentence.iter_tokens() {
                    let surface = token.surface();
                    if surface.trim().is_empty() { continue; }
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
                let tag = token.tags()
                    .first()
                    .and_then(|opt| opt.as_ref())
                    .map(|cow| cow.to_string())
                    .unwrap_or_default();
                if tag.is_empty() {
                    self.config.keep_untagged
                } else {
                    prefixes.iter().any(|p| tag.starts_with(p.as_str()))
                }
            }
        }
    }
}

/// .model.zst のマジックバイト検出（sqlite-vaporetto と同じ判定）。
fn decompress_if_needed(bytes: &[u8]) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    if bytes.starts_with(&[0x28, 0xb5, 0x2f, 0xfd]) {
        let mut decoder = ruzstd::decoding::StreamingDecoder::new(bytes)?;
        let mut out = Vec::new();
        decoder.read_to_end(&mut out)?;
        Ok(out)
    } else {
        Ok(bytes.to_vec())
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

    /// get_tokenizer() が OnceLock でキャッシュされ、2回目以降は初期化コストがゼロに近いことを確認する。
    /// (同一プロセス内でのみ有効)
    #[test]
    fn test_get_tokenizer_cached() {
        use std::time::Instant;

        // モデルが利用できない場合はスキップ
        if EMBEDDED_PREDICTOR_BYTES.is_none()
            && std::env::var("SHIOTSUCHI_MODEL_PATH").is_err()
        {
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
}
