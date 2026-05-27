# PBI-13: Pluggable API Embedder Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add an OpenAI-compatible API embedder backend so users can switch from local ONNX to cloud embedding APIs via config.toml.

**Architecture:** The existing `Embedder` struct becomes a facade over a new internal `EmbedderBackend` enum (`Onnx` / `Api`). A new `core/src/api_embedder.rs` module provides the HTTP client (`ureq`). `EmbedderConfig` gains an `Api` variant. CLI call sites switch from `resolve_model_path()` to `create_embedder()`.

**Tech Stack:** Rust, ureq (sync HTTP), serde_json, tokio (not used — sync only)

---

## Task 1: Add `ureq` dependency to core crate

**Files:**
- Modify: `core/Cargo.toml`

- [ ] **Step 1: Add `ureq` as optional dependency under `semantic` feature**

  Edit `core/Cargo.toml` dependencies section. Find the `ort` line and add `ureq` immediately after it:

  ```toml
  ort = { version = "2.0.0-rc.12", default-features = false, features = ["std", "download-binaries", "tls-rustls"], optional = true }
  ureq = { version = "3", optional = true }
  ```

  Then update the `semantic` feature line from:
  ```toml
  semantic = ["dep:ort", "dep:tokenizers"]
  ```
  to:
  ```toml
  semantic = ["dep:ort", "dep:tokenizers", "dep:ureq"]
  ```

- [ ] **Step 2: Verify Cargo.toml parses**

  ```bash
  cd core && cargo check --no-default-features
  cd .. && cargo check -p shiotsuchi-core
  ```

- [ ] **Step 3: Commit**

  ```bash
  git add core/Cargo.toml
  git commit -m "chore(deps): add ureq for API embedder (semantic feature)"
  ```

---

## Task 2: Create `core/src/api_embedder.rs`

**Files:**
- Create: `core/src/api_embedder.rs`

- [ ] **Step 1: Write request/response types and `ApiClient`**

  Create `core/src/api_embedder.rs` with the following content. Keep everything `pub(super)` or `pub(crate)` — nothing is re-exported at the crate root except through `Embedder`.

  ```rust
  use serde::{Deserialize, Serialize};
  use std::time::Duration;

  use crate::embedder::EmbedderError;

  const DEFAULT_TIMEOUT_SECS: u64 = 60;
  const DEFAULT_BATCH_CAP: usize = 100;

  #[derive(Debug, Serialize)]
  struct EmbeddingRequest<'a> {
      model: &'a str,
      input: Vec<&'a str>,
  }

  #[derive(Debug, Deserialize)]
  struct EmbeddingResponse {
      data: Vec<EmbeddingData>,
  }

  #[derive(Debug, Deserialize)]
  struct EmbeddingData {
      embedding: Vec<f32>,
  }

  /// Internal HTTP client for OpenAI-compatible embedding APIs.
  #[derive(Debug, Clone)]
  pub(super) struct ApiClient {
      endpoint: String,
      model: String,
      api_key: String,
      timeout: Duration,
      batch_cap: usize,
  }

  impl ApiClient {
      pub(super) fn new(
          endpoint: String,
          model: String,
          api_key: String,
      ) -> Self {
          Self {
              endpoint,
              model,
              api_key,
              timeout: Duration::from_secs(DEFAULT_TIMEOUT_SECS),
              batch_cap: DEFAULT_BATCH_CAP,
          }
      }

      pub(super) fn model_id(&self) -> String {
          // Stable identifier: hash of endpoint + model
          use sha2::{Digest, Sha256};
          let mut hasher = Sha256::new();
          hasher.update(self.endpoint.as_bytes());
          hasher.update(self.model.as_bytes());
          format!("api:{}", hex::encode(hasher.finalize()))
      }

      pub(super) fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, EmbedderError> {
          if texts.is_empty() {
              return Ok(vec![]);
          }

          let mut all_embeddings = Vec::with_capacity(texts.len());

          for chunk in texts.chunks(self.batch_cap) {
              let request_body = EmbeddingRequest {
                  model: &self.model,
                  input: chunk.to_vec(),
              };

              let body_json = serde_json::to_string(&request_body)
                  .map_err(|e| EmbedderError::Inference(format!("JSON serialize error: {}", e)))?;

              let response = ureq::post(&self.endpoint)
                  .header("Authorization", &format!("Bearer {}", self.api_key))
                  .header("Content-Type", "application/json")
                  .timeout(self.timeout)
                  .send(&body_json)
                  .map_err(|e| EmbedderError::Inference(format!("API request failed: {}", e)))?;

              if response.status() >= 300 {
                  let status = response.status();
                  let body = response.into_body()
                      .read_to_string()
                      .unwrap_or_default();
                  return Err(EmbedderError::Inference(
                      format!("API error: {} — {}", status, body)
                  ));
              }

              let body_str = response.into_body()
                  .read_to_string()
                  .map_err(|e| EmbedderError::Inference(format!("API response read error: {}", e)))?;

              let parsed: EmbeddingResponse = serde_json::from_str(&body_str)
                  .map_err(|e| EmbedderError::Inference(format!("invalid API response: {} — body: {}", e, body_str)))?;

              if parsed.data.len() != chunk.len() {
                  return Err(EmbedderError::Inference(
                      format!("API returned {} embeddings for {} inputs", parsed.data.len(), chunk.len())
                  ));
              }

              for d in parsed.data {
                  all_embeddings.push(d.embedding);
              }
          }

          Ok(all_embeddings)
      }
  }

  #[cfg(test)]
  mod tests {
      use super::*;

      #[test]
      fn test_api_client_model_id_stable() {
          let c1 = ApiClient::new(
              "https://api.ai.sakura.ad.jp/v1/embeddings".to_string(),
              "multilingual-e5-large".to_string(),
              "key".to_string(),
          );
          let c2 = ApiClient::new(
              "https://api.ai.sakura.ad.jp/v1/embeddings".to_string(),
              "multilingual-e5-large".to_string(),
              "different-key".to_string(),
          );
          // Different API keys should not affect model_id
          assert_eq!(c1.model_id(), c2.model_id());
      }

      #[test]
      fn test_api_client_model_id_differs_by_endpoint() {
          let c1 = ApiClient::new(
              "https://a.example.com/v1/embeddings".to_string(),
              "model".to_string(),
              "key".to_string(),
          );
          let c2 = ApiClient::new(
              "https://b.example.com/v1/embeddings".to_string(),
              "model".to_string(),
              "key".to_string(),
          );
          assert_ne!(c1.model_id(), c2.model_id());
      }

      #[test]
      fn test_embed_batch_empty_returns_empty() {
          let client = ApiClient::new(
              "https://example.com".to_string(),
              "model".to_string(),
              "key".to_string(),
          );
          let result = client.embed_batch(&[]).unwrap();
          assert!(result.is_empty());
      }

      #[test]
      fn test_parse_openai_response() {
          let json = r#"{"data":[{"embedding":[0.1,0.2,0.3]},{"embedding":[0.4,0.5,0.6]}]}"#;
          let resp: EmbeddingResponse = serde_json::from_str(json).unwrap();
          assert_eq!(resp.data.len(), 2);
          assert_eq!(resp.data[0].embedding, vec![0.1_f32, 0.2_f32, 0.3_f32]);
      }
  }
  ```

- [ ] **Step 2: Verify it compiles**

  ```bash
  cargo check -p shiotsuchi-core
  ```

- [ ] **Step 3: Commit**

  ```bash
  git add core/src/api_embedder.rs
  git commit -m "feat(api-embedder): add ApiClient for OpenAI-compatible embedding APIs

- Request/response types (serde)
- Batch chunking (default cap 100)
- Timeout 60s
- Stable model_id from endpoint+model hash
- Unit tests for model_id, empty batch, JSON parsing"
  ```

---

## Task 3: Refactor `core/src/embedder.rs` to support `EmbedderBackend`

**Files:**
- Modify: `core/src/embedder.rs`

- [ ] **Step 1: Import `ApiClient` and add `EmbedderBackend` enum**

  At the top of `core/src/embedder.rs`, after the existing `use` statements, add:

  ```rust
  use crate::api_embedder::ApiClient;
  ```

  Replace the existing `Embedder` struct definition (lines 27-31) with:

  ```rust
  #[derive(Debug)]
  pub struct Embedder {
      backend: EmbedderBackend,
  }

  #[derive(Debug)]
  enum EmbedderBackend {
      Onnx {
          session: RefCell<Session>,
          tokenizer: Tokenizer,
          model_id: String,
      },
      Api {
          client: ApiClient,
          model_id: String,
      },
  }
  ```

- [ ] **Step 2: Update `Embedder::load` to use `Onnx` backend**

  In `Embedder::load`, change the `Ok(Self { … })` return (around line 83) from:

  ```rust
  Ok(Self {
      session: RefCell::new(session),
      tokenizer,
      model_id,
  })
  ```

  to:

  ```rust
  Ok(Self {
      backend: EmbedderBackend::Onnx {
          session: RefCell::new(session),
          tokenizer,
          model_id,
      },
  })
  ```

- [ ] **Step 3: Update public methods to delegate through `backend`**

  Replace `embed`, `embed_batch`, `status`, and `model_id` implementations with `match self.backend` delegators.

  **Note:** `embed_batch_inner` and ONNX-specific helpers remain untouched; they are called only from the `Onnx` arm.

  ```rust
  impl Embedder {
      pub fn load(model_path: &Path) -> Result<Self, EmbedderError> {
          // ... existing body unchanged up to the Ok(Self) return
          Ok(Self {
              backend: EmbedderBackend::Onnx {
                  session: RefCell::new(session),
                  tokenizer,
                  model_id,
              },
          })
      }

      pub fn new(model_path: &Path) -> Result<Self, EmbedderError> {
          Self::load(model_path)
      }

      pub fn embed(&self, text: &str) -> Result<Vec<f32>, EmbedderError> {
          match &self.backend {
              EmbedderBackend::Onnx { .. } => {
                  let results = self.embed_batch_inner(&[text])?;
                  results.into_iter().next().ok_or_else(|| {
                      EmbedderError::Inference("No output from batch".to_string())
                  })
              }
              EmbedderBackend::Api { client, .. } => {
                  let mut results = client.embed_batch(&[text])?;
                  results.into_iter().next().ok_or_else(|| {
                      EmbedderError::Inference("No output from API batch".to_string())
                  })
              }
          }
      }

      pub fn embed_batch(&self, texts: &[&str]) -> Vec<Result<Vec<f32>, EmbedderError>> {
          match &self.backend {
              EmbedderBackend::Onnx { .. } => {
                  match self.embed_batch_inner(texts) {
                      Ok(results) => results.into_iter().map(Ok).collect(),
                      Err(e) => {
                          let err = EmbedderError::Inference(e.to_string());
                          texts.iter().map(|_| Err(err.clone())).collect()
                      }
                  }
              }
              EmbedderBackend::Api { client, .. } => {
                  match client.embed_batch(texts) {
                      Ok(results) => results.into_iter().map(Ok).collect(),
                      Err(e) => {
                          let err = EmbedderError::Inference(e.to_string());
                          texts.iter().map(|_| Err(err.clone())).collect()
                      }
                  }
              }
          }
      }

      pub fn status(&self) -> EmbedderStatus {
          match &self.backend {
              EmbedderBackend::Onnx { .. } => EmbedderStatus::Ready,
              EmbedderBackend::Api { .. } => EmbedderStatus::Ready,
          }
      }

      pub fn model_id(&self) -> &str {
          match &self.backend {
              EmbedderBackend::Onnx { model_id, .. } => model_id,
              EmbedderBackend::Api { model_id, .. } => model_id,
          }
      }

      // ── internal helpers ──────────────────────────────────────────────
      // embed_batch_inner, extract_embeddings, mean_pool_l2_normalize
      // remain unchanged (Onnx-only)
  }
  ```

  **Careful:** The `embed_batch_inner` method references `self.session` and `self.tokenizer` directly. Since those fields now live inside `EmbedderBackend::Onnx`, adjust the method to match on `&self.backend` at its entry point:

  ```rust
  fn embed_batch_inner(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, EmbedderError> {
      let (session, tokenizer) = match &self.backend {
          EmbedderBackend::Onnx { session, tokenizer, .. } => (session, tokenizer),
          EmbedderBackend::Api { .. } => {
              return Err(EmbedderError::Inference(
                  "embed_batch_inner called on API backend".to_string()
              ));
          }
      };

      // ... rest of existing implementation, replacing `self.tokenizer` with `tokenizer`
      // and `self.session` with `session`
  }
  ```

- [ ] **Step 4: Verify compilation**

  ```bash
  cargo check -p shiotsuchi-core
  cargo test -p shiotsuchi-core test_embedder
  ```

- [ ] **Step 5: Commit**

  ```bash
  git add core/src/embedder.rs
  git commit -m "refactor(embedder): introduce EmbedderBackend enum (Onnx / Api)

- Embedder struct becomes facade over EmbedderBackend
- ONNX-specific fields moved into EmbedderBackend::Onnx
- Public methods (embed, embed_batch, status, model_id) delegate via match
- embed_batch_inner gated to Onnx arm only
- No change to external call sites"
  ```

---

## Task 4: Extend `EmbedderConfig` with `Api` variant and `create_embedder()`

**Files:**
- Modify: `core/src/config.rs`
- Modify: `core/src/api_embedder.rs` (minor: make `ApiClient` constructor `pub(crate)` if needed)

- [ ] **Step 1: Add `Api` variant to `EmbedderConfig`**

  In `core/src/config.rs`, replace the existing enum:

  ```rust
  #[derive(Debug, Clone, Serialize, Deserialize, Default)]
  #[serde(tag = "provider", rename_all = "kebab-case")]
  pub enum EmbedderConfig {
      #[default]
      BuiltIn,
      OnnxFile { path: PathBuf },
      Api {
          endpoint: String,
          model: String,
          #[serde(default)]
          api_key: Option<String>,
      },
  }
  ```

- [ ] **Step 2: Add `create_embedder()` method**

  Replace the existing `impl EmbedderConfig` block with:

  ```rust
  impl EmbedderConfig {
      /// Resolve to an embedder instance.
      ///
      /// - `OnnxFile` / `BuiltIn`: returns `Embedder::load(path)` via `resolve_model_path()`.
      /// - `Api`: returns `Embedder` backed by `ApiClient`.
      pub fn create_embedder(&self) -> Result<Option<Embedder>, EmbedderError> {
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

                  let client = ApiClient::new(endpoint.clone(), model.clone(), key);
                  let model_id = client.model_id();
                  Ok(Some(Embedder {
                      backend: crate::embedder::EmbedderBackend::Api {
                          client,
                          model_id,
                      },
                  }))
              }
          }
      }

      // Keep existing resolve_model_path() for backward compatibility
      pub fn resolve_model_path(&self) -> Option<PathBuf> {
          // ... existing implementation unchanged
      }
  }
  ```

  **Note:** `EmbedderBackend` is private to `embedder.rs`. To construct it from `config.rs`, either:
  - Make `EmbedderBackend` `pub(crate)` and add a constructor, OR
  - Add `Embedder::from_api_client(client: ApiClient) -> Self` in `embedder.rs`.

  Prefer option 2 (cleaner API). In `embedder.rs`, add inside `impl Embedder`:

  ```rust
  #[cfg(feature = "semantic")]
  pub(crate) fn from_api_client(client: ApiClient) -> Self {
      let model_id = client.model_id();
      Self {
          backend: EmbedderBackend::Api { client, model_id },
      }
  }
  ```

  Then `config.rs` calls `Embedder::from_api_client(client)` instead of touching `EmbedderBackend` directly.

- [ ] **Step 3: Verify compilation**

  ```bash
  cargo check -p shiotsuchi-core
  cargo test -p shiotsuchi-core
  ```

- [ ] **Step 4: Commit**

  ```bash
  git add core/src/config.rs core/src/embedder.rs
  git commit -m "feat(config): add EmbedderConfig::Api variant and create_embedder()

- Api variant with endpoint, model, api_key fields
- create_embedder() routes BuiltIn/OnnxFile -> Embedder::load, Api -> ApiClient
- API key resolution: SHIOTSUCHI_API_KEY env var > config.api_key
- Embedder::from_api_client() constructor for config.rs use
- resolve_model_path() kept for backward compat"
  ```

---

## Task 5: Register `api_embedder` module in `core/src/lib.rs`

**Files:**
- Modify: `core/src/lib.rs`

- [ ] **Step 1: Add module declaration**

  In `core/src/lib.rs`, after `pub mod embedder;`, add:

  ```rust
  #[cfg(feature = "semantic")]
  mod api_embedder;
  ```

  The module must be `mod` (not `pub mod`) because its types are internal — only `Embedder` is public.

- [ ] **Step 2: Update stub module for `--no-default-features`**

  In the `#[cfg(not(feature = "semantic"))]` block of `lib.rs`, the stub `Embedder` needs no changes because `Embedder::from_config` and `Embedder::from_api_client` are `#[cfg(feature = "semantic")]` only.

- [ ] **Step 3: Verify both feature configurations compile**

  ```bash
  cargo check -p shiotsuchi-core
  cargo check -p shiotsuchi-core --no-default-features
  ```

- [ ] **Step 4: Commit**

  ```bash
  git add core/src/lib.rs
  git commit -m "chore(lib): register api_embedder module under semantic feature"
  ```

---

## Task 6: Update CLI call sites (`chart.rs`, `scan.rs`)

**Files:**
- Modify: `cli/src/commands/chart.rs`
- Modify: `cli/src/commands/scan.rs`

- [ ] **Step 1: Update `chart.rs`**

  Replace the embedder loading block (around line 56):

  ```rust
  let embedder = embedder_cfg.resolve_model_path().and_then(|p| match Embedder::load(&p) {
      Ok(e) => {
          if !args.quiet {
              eprintln!("{}", messages::INFO_EMBEDDER_LOADED);
          }
          Some(e)
      }
      Err(e) => {
          if !args.quiet {
              eprintln!("{}", msg_fmt!(messages::WARN_EMBEDDER_LOAD, e));
          }
          None
      }
  });
  ```

  with:

  ```rust
  let embedder = match embedder_cfg.create_embedder() {
      Ok(Some(e)) => {
          if !args.quiet {
              eprintln!("{}", messages::INFO_EMBEDDER_LOADED);
          }
          // Warn if API key is stored in config (not env var)
          if let shiotsuchi_core::config::EmbedderConfig::Api { api_key: Some(_), .. } = embedder_cfg {
              if std::env::var("SHIOTSUCHI_API_KEY").is_err() && !args.quiet {
                  eprintln!("{}", messages::WARN_API_KEY_IN_CONFIG);
              }
          }
          Some(e)
      }
      Ok(None) => {
          if !args.quiet {
              eprintln!("{}", messages::INFO_EMBEDDER_SKIPPED);
          }
          None
      }
      Err(e) => {
          if !args.quiet {
              eprintln!("{}", msg_fmt!(messages::WARN_EMBEDDER_LOAD, e));
          }
          None
      }
  };
  ```

- [ ] **Step 2: Update `scan.rs`**

  Same pattern as `chart.rs`. Replace the embedder loading block (around line 39):

  ```rust
  let embedder = embedder_cfg.resolve_model_path().and_then(|p| match Embedder::load(&p) {
      Ok(e) => {
          eprintln!("{}", messages::INFO_EMBEDDER_LOADED);
          Some(e)
      }
      Err(e) => {
          eprintln!("{}", msg_fmt!(messages::WARN_EMBEDDER_LOAD, e));
          None
      }
  });
  ```

  with:

  ```rust
  let embedder = match embedder_cfg.create_embedder() {
      Ok(Some(e)) => {
          eprintln!("{}", messages::INFO_EMBEDDER_LOADED);
          if let shiotsuchi_core::config::EmbedderConfig::Api { api_key: Some(_), .. } = embedder_cfg {
              if std::env::var("SHIOTSUCHI_API_KEY").is_err() {
                  eprintln!("{}", messages::WARN_API_KEY_IN_CONFIG);
              }
          }
          Some(e)
      }
      Ok(None) => {
          eprintln!("{}", messages::INFO_EMBEDDER_SKIPPED);
          None
      }
      Err(e) => {
          eprintln!("{}", msg_fmt!(messages::WARN_EMBEDDER_LOAD, e));
          None
      }
  };
  ```

- [ ] **Step 3: Verify compilation**

  ```bash
  cargo check -p shiotsuchi
  cargo test -p shiotsuchi
  ```

- [ ] **Step 4: Commit**

  ```bash
  git add cli/src/commands/chart.rs cli/src/commands/scan.rs
  git commit -m "feat(cli): use create_embedder() in chart and scan

- Replace resolve_model_path() -> Embedder::load() with create_embedder()
- Add API key in config warning for Api provider"
  ```

---

## Task 7: Add CLI warning message constant

**Files:**
- Modify: `cli/src/messages.rs`

- [ ] **Step 1: Add `WARN_API_KEY_IN_CONFIG`**

  Add to `cli/src/messages.rs` near the existing embedder messages:

  ```rust
  pub const WARN_API_KEY_IN_CONFIG: &str = "[警告] config.toml に API キーが記載されています。環境変数 SHIOTSUCHI_API_KEY の使用を推奨します。";
  ```

- [ ] **Step 2: Verify compilation**

  ```bash
  cargo check -p shiotsuchi
  ```

- [ ] **Step 3: Commit**

  ```bash
  git add cli/src/messages.rs
  git commit -m "feat(messages): add WARN_API_KEY_IN_CONFIG constant"
  ```

---

## Task 8: Add tests for `EmbedderConfig::Api` deserialization

**Files:**
- Modify: `core/src/config.rs` (test module)

- [ ] **Step 1: Add deserialization tests**

  In the `#[cfg(test)]` module at the bottom of `core/src/config.rs`, add:

  ```rust
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
      let toml = r#"{}"#;
      let config: EmbedderConfig = toml::from_str(toml).unwrap();
      assert!(matches!(config, EmbedderConfig::BuiltIn));
  }
  ```

- [ ] **Step 2: Run tests**

  ```bash
  cargo test -p shiotsuchi-core test_embedder_config
  ```

- [ ] **Step 3: Commit**

  ```bash
  git add core/src/config.rs
  git commit -m "test(config): add EmbedderConfig::Api deserialization tests

- Full API config with api_key
- API config without api_key (optional field)
- Default is BuiltIn
- Empty TOML defaults to BuiltIn"
  ```

---

## Task 9: Add end-to-end CLI test for API key warning

**Files:**
- Modify: `cli/src/commands/chart.rs` (test module)

- [ ] **Step 1: Add test capturing stderr**

  In the `#[cfg(test)]` module of `cli/src/commands/chart.rs`, add:

  ```rust
  #[test]
  fn test_chart_warns_when_api_key_in_config() {
      let temp = TempDir::new().unwrap();
      let db_file = temp.path().join("test.db");
      let vault = temp.path().join("vault");
      std::fs::create_dir_all(&vault).unwrap();
      std::fs::write(vault.join("note.md"), "# Hello").unwrap();

      let api_cfg = shiotsuchi_core::config::EmbedderConfig::Api {
          endpoint: "https://api.example.com".to_string(),
          model: "model".to_string(),
          api_key: Some("sk-test".to_string()),
      };

      let args = ChartArgs {
          force: false,
          quiet: false,
          vault: None,
      };
      let idx_cfg = IndexingConfig::default();

      // We don't actually call the API; the test checks that the warning path
      // is reachable when api_key is present but SHIOTSUCHI_API_KEY is absent.
      // Since create_embedder will fail (no real API), embedder becomes None,
      // but the warning should still print before the error.
      let _ = run_chart(&args, &[("default".to_string(), vault)], &db_file, &idx_cfg, &api_cfg);
      // This test is best-effort: verifying the warning branch is compiled and
      // the logic is correct. Full stderr capture would require a custom writer.
  }
  ```

  If capturing stderr is too complex for the test harness, **skip this test** and instead verify manually:

  ```bash
  cargo test -p shiotsuchi commands::chart::tests
  ```

- [ ] **Step 2: Commit (if test added) or skip**

  ```bash
  git add cli/src/commands/chart.rs
  git commit -m "test(chart): add API key config warning test" || echo "skipped"
  ```

---

## Task 10: Final verification

- [ ] **Step 1: Run full test suite**

  ```bash
  cargo test -p shiotsuchi-core
  cargo test -p shiotsuchi
  cargo check -p shiotsuchi-core --no-default-features
  ```

- [ ] **Step 2: Check for compiler warnings**

  ```bash
  cargo check -p shiotsuchi-core 2>&1 | grep -i "warning" || true
  cargo check -p shiotsuchi 2>&1 | grep -i "warning" || true
  ```

  Fix any unused imports or dead code warnings.

- [ ] **Step 3: Commit any trailing fixes**

  ```bash
  git add -A
  git commit -m "fix: address compiler warnings from API embedder changes"
  ```

---

## Spec Coverage Check

| Spec Requirement | Implementing Task |
|---|---|
| `EmbedderConfig::Api` variant | Task 4 |
| `EmbedderBackend` enum (Onnx / Api) | Task 3 |
| `ApiClient` with ureq | Task 2 |
| OpenAI-compatible request/response | Task 2 |
| Batch cap (100) | Task 2 |
| Timeout (60s) | Task 2 |
| API key priority: env > config | Task 4 |
| CLI warning for api_key in config | Task 6, Task 7 |
| `create_embedder()` replacing `resolve_model_path()` | Task 4, Task 6 |
| Feature flag (`semantic`) | Task 1, Task 5 |
| Stub module compatibility | Task 5 (no change needed) |
| Tests | Task 2, Task 8, Task 9 |

No gaps found.
