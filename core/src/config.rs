use crate::paths::default_db_path as core_default_db_path;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

fn is_false(b: &bool) -> bool {
    !b
}

/// Embedding provider configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(tag = "provider", rename_all = "kebab-case")]
pub enum EmbedderConfig {
    /// Use the built-in model resolution (env var / XDG default). This is the default.
    #[default]
    BuiltIn,
    /// Load a specific ONNX model file from disk.
    OnnxFile {
        path: PathBuf,
    },
    /// Use an external OpenAI-compatible embedding API.
    Api {
        endpoint: String,
        model: String,
        #[serde(default)]
        api_key: Option<String>,
    },
}

impl EmbedderConfig {
    /// Resolve to an embedder instance.
    ///
    /// - `OnnxFile` / `BuiltIn`: returns `Embedder::load(path)` via `resolve_model_path()`.
    /// - `Api`: returns `Embedder` backed by `ApiClient`.
    #[cfg(feature = "semantic")]
    pub fn create_embedder(&self, embedding_usage: &EmbeddingUsageConfig) -> Result<Option<crate::embedder::Embedder>, crate::embedder::EmbedderError> {
        use crate::api_embedder::ApiClient;
        use crate::embedder::{Embedder, EmbedderError};

        match self {
            EmbedderConfig::OnnxFile { path } => {
                if path.exists() {
                    Ok(Some(Embedder::load(path)?))
                } else {
                    Ok(None)
                }
            }
            EmbedderConfig::BuiltIn => {
                match self.resolve_model_path() {
                    Some(path) => Ok(Some(Embedder::load(&path)?)),
                    None => Ok(None),
                }
            }
            EmbedderConfig::Api { endpoint, model, api_key } => {
                let key = std::env::var("SHIOTSUCHI_API_KEY")
                    .ok()
                    .or_else(|| api_key.clone())
                    .ok_or_else(|| EmbedderError::Load(
                        "API key not set. Set SHIOTSUCHI_API_KEY or api_key in config".to_string()
                    ))?;

                let usage_tracker = if embedding_usage.enabled {
                    let config_dir = dirs::config_dir()
                        .unwrap_or_else(|| std::path::PathBuf::from("."))
                        .join("shiotsuchi");
                    Some(crate::usage_tracker::UsageTracker::new(
                        &config_dir,
                        embedding_usage.enabled,
                        embedding_usage.monthly_limit,
                    ))
                } else {
                    None
                };

                let client = ApiClient::new(endpoint.clone(), model.clone(), key, usage_tracker);
                Ok(Some(Embedder::from_api_client(client)))
            }
        }
    }

    #[cfg(not(feature = "semantic"))]
    pub fn create_embedder(&self, _embedding_usage: &EmbeddingUsageConfig) -> Result<Option<crate::embedder::Embedder>, crate::embedder::EmbedderError> {
        Err(crate::embedder::EmbedderError::Unavailable(
            "compiled without the 'semantic' feature".into(),
        ))
    }

    /// Resolve to an ONNX model path.
    ///
    /// - `OnnxFile`: returns the configured path if the file exists.
    /// - `BuiltIn`: delegates to `embedder::resolve_model_path(None)`.
    ///
    /// Returns `None` if no model file is found.
    pub fn resolve_model_path(&self) -> Option<PathBuf> {
        match self {
            EmbedderConfig::OnnxFile { path } => Some(path.clone()),
            EmbedderConfig::BuiltIn => {
                // Delegate to the standard resolution logic in the embedder module.
                // We replicate the logic here to avoid a circular dependency between
                // config and embedder modules.
                use std::env;
                if let Ok(val) = env::var("SHIOTSUCHI_EMBED_MODEL_PATH") {
                    let p = PathBuf::from(val);
                    if p.exists() {
                        return Some(p);
                    }
                }
                let xdg_data = if let Some(xdg) = env::var_os("XDG_DATA_HOME") {
                    PathBuf::from(xdg)
                } else if let Some(home) = dirs::home_dir() {
                    home.join(".local").join("share")
                } else {
                    PathBuf::from(".")
                };
                let default_path = xdg_data.join("shiotsuchi").join("model.onnx");
                if default_path.exists() {
                    Some(default_path)
                } else {
                    None
                }
            }
            EmbedderConfig::Api { .. } => None,
        }
    }

    /// Returns true if the API key is configured in config.toml but not via
    /// the SHIOTSUCHI_API_KEY environment variable. Use this to warn users
    /// about the less secure configuration.
    pub fn has_api_key_in_config_but_not_env(&self) -> bool {
        match self {
            EmbedderConfig::Api { api_key: Some(_), .. } => {
                std::env::var("SHIOTSUCHI_API_KEY").is_err()
            }
            _ => false,
        }
    }
}

/// Monthly embedding API usage limit configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct EmbeddingUsageConfig {
    /// Whether usage tracking is enabled. Default: false.
    pub enabled: bool,
    /// Monthly request limit. None = unlimited.
    pub monthly_limit: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct DatabaseConfig {
    pub db_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct VaultEntry {
    pub notes_dir: Option<PathBuf>,
    #[serde(default)]
    pub db_path: Option<PathBuf>,
}

fn xdg_config_home() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            dirs::home_dir()
                .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
                .join(".config")
        })
}

pub fn default_config_path() -> PathBuf {
    xdg_config_home().join("shiotsuchi").join("config.toml")
}

/// Path to the standalone thesaurus file for synonym management.
/// This file is managed by the `shiotsuchi synonym` CLI command.
pub fn thesaurus_path() -> PathBuf {
    xdg_config_home().join("shiotsuchi").join("thesaurus.toml")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct IndexingConfig {
    pub include_extensions: Vec<String>,
    pub exclude_dirs: Vec<String>,
    pub auto_exclude_hidden: bool,
    pub follow_links: bool,
    pub dynamic_threshold: usize,
    /// User-defined dictionary entries for custom tokenization.
    /// Entries are matched case-sensitively against the token stream.
    /// Multi-word entries (e.g., "Amazon Web Services") and single-word
    /// entries that Vaporetto would split (e.g., "ChatGPT") are supported.
    #[serde(default)]
    pub user_dictionary: Vec<String>,
    /// Whether to extract text from PDF files during indexing.
    /// When false, PDF files are indexed with empty content (files still appear in the DB).
    /// Default: true.
    #[serde(default = "default_enable_pdf_extraction")]
    pub enable_pdf_extraction: bool,
    /// Whether to apply backlink count scoring boost to search results.
    /// When true, files with more backlinks get a score boost in search results.
    /// Default: true.
    #[serde(default = "default_backlink_scoring")]
    pub backlink_scoring: bool,
    /// Number of days to retain indexed data. When set, `shiotsuchi prune --expired`
    /// deletes files whose mtime exceeds this threshold. When None (default), no
    /// automatic expiration is performed.
    #[serde(default)]
    pub retention_days: Option<u32>,
    /// Monthly embedding API usage tracking configuration.
    #[serde(default)]
    pub embedding_usage: EmbeddingUsageConfig,
}

fn default_enable_pdf_extraction() -> bool {
    true
}

fn default_backlink_scoring() -> bool {
    true
}

impl Default for IndexingConfig {
    fn default() -> Self {
        Self {
            include_extensions: vec!["md".to_string(), "markdown".to_string()],
            exclude_dirs: vec!["node_modules".to_string()],
            auto_exclude_hidden: true,
            follow_links: false,
            dynamic_threshold: 5,
            user_dictionary: vec![],
            enable_pdf_extraction: true,
            backlink_scoring: true,
            retention_days: None,
            embedding_usage: EmbeddingUsageConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct WatcherConfig {
    pub enabled: bool,
}

impl Default for WatcherConfig {
    fn default() -> Self {
        Self { enabled: true }
    }
}

/// HTTP server configuration for `shiotsuchi serve`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ServerConfig {
    pub port: u16,
    pub host: String,
    pub cors_origins: Vec<String>,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            port: 7171,
            host: "127.0.0.1".to_string(),
            cors_origins: vec!["http://localhost".to_string()],
        }
    }
}

/// Configuration for VLM-based PDF extraction (e.g., scanned PDFs with no embedded text).
/// Requires the `vlm` Cargo feature flag.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct VlmConfig {
    /// Enable VLM-based extraction. When false, all VLM features are skipped (even if API key is set).
    pub enabled: bool,
    /// Whether the user has consented to sending data to the VLM provider.
    /// Once granted, consent is persisted in config.toml.
    #[serde(default, skip_serializing_if = "is_false")]
    pub consent_obtained: bool,
    /// VLM provider name (e.g., "openai", "anthropic", "bedrock", "gemini", "ollama").
    /// Default: "openai"
    pub provider: String,
    /// Explicit endpoint URL override. When empty, the provider's default URL is used.
    pub endpoint: Option<String>,
    /// Model name to use (e.g., "gpt-4.1-nano", "claude-sonnet-4-20250514").
    /// Default: "gpt-4.1-nano"
    pub model: String,
    /// Maximum pages to process per document. None = unlimited.
    pub max_pages_per_doc: Option<usize>,
}

impl Default for VlmConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            consent_obtained: false,
            provider: "openai".to_string(),
            endpoint: None,
            model: "gpt-4.1-nano".to_string(),
            max_pages_per_doc: Some(10),
        }
    }
}

impl VlmConfig {
    /// Resolve the endpoint URL for this VLM configuration.
    /// Returns the explicitly configured endpoint, or the provider's default.
    pub fn resolved_endpoint(&self) -> String {
        self.endpoint.clone().unwrap_or_else(|| resolved_vlm_endpoint_for_provider(&self.provider))
    }

    /// Return the effective max pages per document, capped at a system-level limit.
    /// When `max_pages_per_doc` is `None` or exceeds the hard cap, the cap is used
    /// to prevent runaway API costs from unusually large documents.
    pub fn effective_max_pages_per_doc(&self) -> usize {
        const HARD_CAP: usize = 50;
        match self.max_pages_per_doc {
            Some(n) => n.min(HARD_CAP),
            None => HARD_CAP,
        }
    }
}

fn resolved_vlm_endpoint_for_provider(provider: &str) -> String {
    match provider {
        "openai" => "https://api.openai.com/v1".to_string(),
        "anthropic" => "https://api.anthropic.com/v1".to_string(),
        "ollama" => "http://localhost:11434".to_string(),
        "bedrock" => "https://bedrock-runtime.us-east-1.amazonaws.com".to_string(),
        "gemini" => "https://generativelanguage.googleapis.com/v1beta".to_string(),
        _ => format!("https://{}.example.com/v1", provider),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ShiotsuchiConfig {
    pub database: DatabaseConfig,
    pub vaults: HashMap<String, VaultEntry>,
    pub vault: Option<VaultEntry>,
    pub indexing: IndexingConfig,
    pub watcher: WatcherConfig,
    pub vlm: VlmConfig,
    /// Synonym mappings for search query expansion.
    /// Keys are query tokens, values are lists of synonyms.
    /// Synonyms are OR-expanded in FTS5 queries.
    /// Example: { "AWS" -> ["Amazon Web Services", "アマゾンウェブサービス"] }
    #[serde(default)]
    pub synonyms: HashMap<String, Vec<String>>,
    /// Blend ratio for hybrid search (0.0 = semantic only, 1.0 = FTS only).
    /// Default: 0.5 (equal blend). Can be overridden at runtime with --alpha.
    #[serde(default)]
    pub hybrid_alpha: Option<f64>,
    /// Default vault ID used when `--vault` is not specified on the CLI.
    /// When set, `dive` / `chart` / `scan` will operate on only this vault.
    /// Example config.toml: `vault_default = "work"`
    #[serde(default)]
    pub vault_default: Option<String>,
    /// Minimum score threshold for search results.
    /// FTS: excludes results with score > threshold (lower BM25 = more relevant).
    /// Vec: excludes results with distance > threshold.
    /// Hybrid: excludes results with RRF score < threshold.
    /// Example config.toml: `semantic_threshold = 0.75`
    #[serde(default)]
    pub semantic_threshold: Option<f64>,
    /// Embedding model configuration.
    /// Defaults to built-in model resolution (env var / XDG default).
    #[serde(default)]
    pub embedder: EmbedderConfig,
    /// HTTP server configuration.
    #[serde(default)]
    pub server: ServerConfig,
    /// Sensitive data detection and masking configuration.
    #[serde(default)]
    pub sensitive_data: crate::sensitive::SensitiveDataConfig,
    /// Top-level embedding usage configuration (threaded into IndexingConfig at runtime).
    #[serde(default)]
    pub embedding_usage: EmbeddingUsageConfig,
}

impl ShiotsuchiConfig {
    pub fn resolved_vaults(&self) -> Vec<(String, PathBuf)> {
        let mut vaults: Vec<(String, PathBuf)> = Vec::new();

        if let Some(ref v) = self.vault {
            if let Some(ref dir) = v.notes_dir {
                vaults.push(("default".to_string(), dir.clone()));
            }
        }

        for (name, entry) in &self.vaults {
            if let Some(ref dir) = entry.notes_dir {
                vaults.push((name.clone(), dir.clone()));
            }
        }

        if vaults.is_empty() {
            vaults.push(("default".to_string(), PathBuf::from(".")));
            eprintln!("[warn] No vaults configured. Using current directory as 'default' vault.");
        }

        vaults
    }

    pub fn resolved_db_path(&self) -> PathBuf {
        self.database
            .db_path
            .clone()
            .or_else(|| self.vault.as_ref().and_then(|v| v.db_path.clone()))
            .unwrap_or_else(core_default_db_path)
    }

    pub fn load_from(path: &Path) -> Result<Self, config::ConfigError> {
        config::Config::builder()
            .add_source(config::File::from(path))
            .build()?
            .try_deserialize()
    }

    pub fn load() -> Self {
        let default_path = xdg_config_home().join("shiotsuchi").join("config.toml");
        let mut cfg = if default_path.exists() {
            let cfg = Self::load_from(&default_path).unwrap_or_else(|e| {
                eprintln!(
                    "Warning: failed to load config from {}: {}. Using defaults.",
                    default_path.display(),
                    e
                );
                Self::default()
            });
            if cfg.vault.is_some() {
                eprintln!(
                    "[hint] Your config uses the old [vault] format. Run 'shiotsuchi config-migrate' to upgrade."
                );
            }
            cfg
        } else {
            Self::default()
        };

        // Merge thesaurus.toml into synonyms (thesaurus entries take priority).
        let thes_path = thesaurus_path();
        if thes_path.exists() {
            match Self::load_synonyms_from(&thes_path) {
                Ok(thesaurus_syns) => {
                    for (word, syns) in thesaurus_syns {
                        cfg.synonyms.insert(word, syns);
                    }
                }
                Err(e) => {
                    eprintln!(
                        "Warning: failed to load thesaurus from {}: {}",
                        thes_path.display(),
                        e
                    );
                }
            }
        }

        cfg
    }

    /// Load synonyms from a thesaurus file. The file is a flat TOML table
    /// mapping words to arrays of synonym strings:
    /// ```toml
    /// AWS = ["Amazon Web Services", "アマゾンウェブサービス"]
    /// k8s = ["Kubernetes"]
    /// ```
    pub fn load_synonyms_from(path: &Path) -> Result<HashMap<String, Vec<String>>, Box<dyn std::error::Error>> {
        let content = std::fs::read_to_string(path)?;
        let syns: HashMap<String, Vec<String>> = toml::from_str(&content)?;
        Ok(syns)
    }

    /// Save this config to a TOML file with restricted permissions (0o600 on Unix).
    pub fn save_to(&self, path: &Path) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let toml_str = toml::to_string_pretty(self)?;
        let tmp_path = path.with_extension("toml.tmp");
        std::fs::write(&tmp_path, toml_str)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&tmp_path, std::fs::Permissions::from_mode(0o600))?;
        }
        std::fs::rename(&tmp_path, path)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_embedder_config_api_deserialization() {
        let toml = r#"
            provider = "api"
            endpoint = "https://api.ai.sakura.ad.jp/v1/embeddings"
            model = "multilingual-e5-large"
            api_key = "sk-test"
        "#;
        let config: EmbedderConfig = toml::from_str(toml).unwrap();
        match config {
            EmbedderConfig::Api { endpoint, model, api_key } => {
                assert_eq!(endpoint, "https://api.ai.sakura.ad.jp/v1/embeddings");
                assert_eq!(model, "multilingual-e5-large");
                assert_eq!(api_key, Some("sk-test".to_string()));
            }
            other => panic!("Expected Api variant, got {:?}", other),
        }
    }

    #[test]
    fn test_embedder_config_api_without_api_key() {
        let toml = r#"
            provider = "api"
            endpoint = "https://api.example.com/v1/embeddings"
            model = "text-embedding-3-small"
        "#;
        let config: EmbedderConfig = toml::from_str(toml).unwrap();
        match config {
            EmbedderConfig::Api { api_key, .. } => {
                assert_eq!(api_key, None);
            }
            other => panic!("Expected Api variant, got {:?}", other),
        }
    }

    #[test]
    fn test_embedder_config_default_is_builtin() {
        let config: EmbedderConfig = EmbedderConfig::default();
        assert!(matches!(config, EmbedderConfig::BuiltIn));
    }

    #[test]
    fn test_embedder_config_builtin_omitted_provider() {
        let toml = r#""#;
        let config: ShiotsuchiConfig = toml::from_str(toml).unwrap();
        assert!(matches!(config.embedder, EmbedderConfig::BuiltIn));
    }

    #[test]
    fn test_has_api_key_in_config_but_not_env_builtin_returns_false() {
        let cfg = EmbedderConfig::BuiltIn;
        assert!(!cfg.has_api_key_in_config_but_not_env());
    }

    #[test]
    fn test_has_api_key_in_config_but_not_env_onnx_returns_false() {
        let cfg = EmbedderConfig::OnnxFile { path: PathBuf::from("/tmp/model.onnx") };
        assert!(!cfg.has_api_key_in_config_but_not_env());
    }

    #[test]
    fn test_has_api_key_in_config_but_not_env_api_no_key_returns_false() {
        let cfg = EmbedderConfig::Api {
            endpoint: "https://example.com".to_string(),
            model: "model".to_string(),
            api_key: None,
        };
        assert!(!cfg.has_api_key_in_config_but_not_env());
    }

    #[test]
    fn test_has_api_key_in_config_but_not_env_with_env_key_returns_false() {
        let old_env = std::env::var_os("SHIOTSUCHI_API_KEY");
        std::env::set_var("SHIOTSUCHI_API_KEY", "sk-env-test");
        let cfg = EmbedderConfig::Api {
            endpoint: "https://example.com".to_string(),
            model: "model".to_string(),
            api_key: Some("sk-config".to_string()),
        };
        assert!(!cfg.has_api_key_in_config_but_not_env());
        // Restore previous env var state
        match old_env {
            Some(v) => std::env::set_var("SHIOTSUCHI_API_KEY", v),
            None => std::env::remove_var("SHIOTSUCHI_API_KEY"),
        }
    }

    #[test]
    fn test_indexing_config_enable_pdf_extraction_default_is_true() {
        let cfg = IndexingConfig::default();
        assert!(cfg.enable_pdf_extraction, "should default to true");
    }

    #[test]
    fn test_indexing_config_enable_pdf_extraction_deserialize_false() {
        let toml = r#"
            [indexing]
            enable_pdf_extraction = false
        "#;
        let config: ShiotsuchiConfig = toml::from_str(toml).unwrap();
        assert!(!config.indexing.enable_pdf_extraction);
    }

    #[test]
    fn test_indexing_config_enable_pdf_extraction_omitted_is_true() {
        // Backward compat: old configs without the field should still parse
        let toml = r"
            [indexing]
            include_extensions = ['md']
        ";
        let config: ShiotsuchiConfig = toml::from_str(toml).unwrap();
        assert!(config.indexing.enable_pdf_extraction, "omitted field should default to true");
    }

    #[test]
    fn test_backlink_scoring_default_is_true() {
        let cfg = IndexingConfig::default();
        assert!(cfg.backlink_scoring, "backlink_scoring should default to true");
    }

    #[test]
    fn test_backlink_scoring_deserialize_false() {
        let toml = r#"
            [indexing]
            backlink_scoring = false
        "#;
        let config: ShiotsuchiConfig = toml::from_str(toml).unwrap();
        assert!(!config.indexing.backlink_scoring);
    }

    #[test]
    fn test_backlink_scoring_omitted_is_true() {
        let toml = r"
            [indexing]
            include_extensions = ['md']
        ";
        let config: ShiotsuchiConfig = toml::from_str(toml).unwrap();
        assert!(config.indexing.backlink_scoring, "omitted backlink_scoring should default to true");
    }

    #[test]
    fn test_exclude_patterns_rename_backward_compat_denied() {
        // deny_unknown_fields on IndexingConfig rejects the old key name
        let toml = r#"
            [indexing]
            exclude_patterns = ["build"]
        "#;
        let result: Result<ShiotsuchiConfig, _> = toml::from_str(toml);
        assert!(result.is_err(), "old `exclude_patterns` key must be rejected: {:?}", result.err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("exclude_patterns") || err.contains("unknown"), "error must mention the old key name");
    }

    #[test]
    fn test_exclude_dirs_accepts_new_key() {
        let toml = r#"
            [indexing]
            exclude_dirs = ["build"]
        "#;
        let config: ShiotsuchiConfig = toml::from_str(toml).unwrap();
        assert_eq!(config.indexing.exclude_dirs, vec!["build"]);
    }

    #[test]
    fn test_exclude_dirs_default_deserialize() {
        let toml = r"
            [indexing]
        ";
        let config: ShiotsuchiConfig = toml::from_str(toml).unwrap();
        assert_eq!(config.indexing.exclude_dirs, vec!["node_modules"]);
    }

    #[test]
    fn test_indexing_config_exclude_dirs_accepts_new_key_only() {
        // Verify that deny_unknown_fields is active for IndexingConfig
        let toml = r#"
            [indexing]
            exclude_dirs = ["dist"]
        "#;
        let config: ShiotsuchiConfig = toml::from_str(toml).unwrap();
        assert_eq!(config.indexing.exclude_dirs, vec!["dist"]);
    }
}
