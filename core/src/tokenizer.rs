include!(concat!(env!("OUT_DIR"), "/embedded_model.rs"));

use std::io::Read;
use thiserror::Error;
use vaporetto::{Model, Predictor, Sentence};

#[derive(Error, Debug)]
pub enum TokenizerError {
    #[error("モデルが見つかりません: SHIOTSUCHI_MODEL_PATH を設定するか、SHIOTSUCHI_EMBED_MODEL 付きで再ビルドしてください")]
    NoModel,
    #[error("モデルロード失敗: {0}")]
    ModelLoad(String),
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
    /// モデルロード優先順位（sqlite-vaporetto のモデル設定階層と同じ）:
    /// 1. EMBEDDED_MODEL_BYTES（build.rs で include_bytes! 埋め込み）
    /// 2. SHIOTSUCHI_MODEL_PATH 環境変数
    /// 3. どちらもなければ TokenizerError::NoModel
    pub fn new(config: TokenizerConfig) -> Result<Self, TokenizerError> {
        let bytes_owned: Vec<u8>;
        let model_bytes: &[u8] = if let Some(embedded) = EMBEDDED_MODEL_BYTES {
            embedded
        } else if let Ok(path) = std::env::var("SHIOTSUCHI_MODEL_PATH") {
            let raw = std::fs::read(&path)
                .map_err(|e| TokenizerError::ModelLoad(format!("{}: {}", path, e)))?;
            bytes_owned = raw;
            &bytes_owned
        } else {
            return Err(TokenizerError::NoModel);
        };

        let model_data = decompress_if_needed(model_bytes)
            .map_err(|e| TokenizerError::ModelLoad(e.to_string()))?;
        let model = Model::read(model_data.as_slice())
            .map_err(|e| TokenizerError::ModelLoad(e.to_string()))?;
        let predictor = Predictor::new(model, false)
            .map_err(|e| TokenizerError::ModelLoad(e.to_string()))?;

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
}
