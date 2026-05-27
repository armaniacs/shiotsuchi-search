# PBI-13 Design: Pluggable Embedding Model (API Provider)

**Date:** 2026-05-28
**Feature:** OpenAI-compatible API embedder (`ApiEmbedder`)

## Goal

Enable users to switch from the built-in ONNX local model to any OpenAI-compatible embedding API (e.g. Sakura AI `multilingual-e5-large`, OpenAI `text-embedding-3-small`, etc.) via `config.toml`. The change must be transparent to consumers (`indexer.rs`, `search.rs`, watcher) — they continue calling `embed()` / `embed_batch()` / `model_id()` unchanged.

## Context

- `core/src/embedder.rs` currently provides a single `Embedder` struct backed by ONNX Runtime (`ort`).
- `core/src/config.rs` defines `EmbedderConfig` with two variants: `BuiltIn` and `OnnxFile`.
- CLI `chart` / `scan` receive `&EmbedderConfig` and call `embedder_cfg.resolve_model_path()` → `Embedder::load()`.
- The `semantic` Cargo feature gates all vector functionality; a stub `Embedder` exists for `--no-default-features` builds.

## Design Decisions

### 1. `EmbedderConfig` extended with `Api` variant

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "provider", rename_all = "kebab-case")]
pub enum EmbedderConfig {
    #[default]
    BuiltIn,
    OnnxFile { path: PathBuf },
    Api {
        endpoint: String,          // e.g. "https://api.ai.sakura.ad.jp/v1/embeddings"
        model: String,             // e.g. "multilingual-e5-large"
        #[serde(default)]
        api_key: Option<String>,   // fallback when env var is absent
    },
}
```

TOML example:

```toml
[embedder]
provider = "api"
endpoint = "https://api.ai.sakura.ad.jp/v1/embeddings"
model = "multilingual-e5-large"
api_key = "sk-..."   # optional; SHIOTSUCHI_API_KEY env var takes precedence
```

### 2. `Embedder` struct gains an internal backend enum

Instead of introducing a new wrapper type, the existing `Embedder` struct is kept as the **single public type** consumed by `indexer.rs`, `search.rs`, and the watcher. Internally it switches on a new `EmbedderBackend` enum:

```rust
pub struct Embedder {
    backend: EmbedderBackend,
}

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

Public methods (`embed`, `embed_batch`, `model_id`, `status`) become `match self.backend { … }` delegators. This keeps all call sites (indexer, search, watcher, MCP) unchanged — they still hold `Option<&Embedder>`.

`Embedder::load(model_path)` is kept as an ONNX-specific constructor and is **unchanged**. A new constructor `Embedder::from_config(cfg: &EmbedderConfig) -> Result<Self, EmbedderError>` is added, routing to:
- `BuiltIn` / `OnnxFile` → `Embedder::load(path)`
- `Api` → `Embedder { backend: EmbedderBackend::Api { … } }`

### 3. `ApiEmbedder` internals (`core/src/api_embedder.rs`)

`ApiClient` (internal struct, not exposed) handles the HTTP plumbing:
- Uses `ureq` (sync, lightweight). Added to `core/Cargo.toml` as `ureq = { version = "3", optional = true }` under the `semantic` feature.
- OpenAI-compatible request/response JSON format.
- Batch size capped at 100 texts per request (default; configurable in `EmbedderConfig::Api`).
- Error mapping: HTTP non-2xx → `EmbedderError::Inference` with body text.
- Timeout: 60 seconds per request (configurable in `EmbedderConfig::Api`).

Request payload:

```json
{
  "model": "multilingual-e5-large",
  "input": ["text1", "text2"]
}
```

Response parsing extracts `data[].embedding` float array.

### 4. API key resolution (secure by default)

Priority:
1. `SHIOTSUCHI_API_KEY` environment variable
2. `EmbedderConfig::Api.api_key` field (fallback)

When `api_key` is read from config, CLI prints:

```
[警告] config.toml に API キーが記載されています。環境変数 SHIOTSUCHI_API_KEY の使用を推奨します。
```

### 5. Configuration API changes

- `EmbedderConfig` gains `create_embedder()` → `Result<Option<Embedder>, EmbedderError>`.
- `resolve_model_path()` is kept for backward compatibility but marked `#[deprecated]`.
- CLI call sites (`chart.rs`, `scan.rs`) updated from:

```rust
let embedder = embedder_cfg.resolve_model_path().and_then(|p| Embedder::load(&p).ok());
```

to:

```rust
let embedder = embedder_cfg.create_embedder().ok().flatten();
```

### 6. Feature flag compatibility

- `ApiEmbedder` / `ureq` are included only when `semantic` feature is enabled.
- Stub `embedder` module (`--no-default-features`) stays unchanged — `Embedder::from_config()` returns `Unavailable` for `Api` variant, same as `OnnxFile`.

## Error Handling

| Scenario | Behavior |
|----------|----------|
| HTTP 4xx/5xx | `EmbedderError::Inference("API error: {status} — {body}")` |
| Timeout (`ureq`) | `EmbedderError::Inference("API request timed out")` |
| Malformed JSON response | `EmbedderError::Inference("invalid API response: {detail}")` |
| Missing API key | `EmbedderError::Load("API key not set. Set SHIOTSUCHI_API_KEY or api_key in config")` |
| Empty batch | Return `Ok(vec![])` (same as local) |

## Testing Strategy

| Test | Location |
|------|----------|
| `ApiClient` parses OpenAI response JSON | `core/src/api_embedder.rs` (unit test with hardcoded JSON) |
| `ApiClient` serializes request correctly | `core/src/api_embedder.rs` (unit test inspecting request body) |
| `EmbedderConfig::Api` deserializes from TOML | `core/src/config.rs` tests |
| `EmbedderBackend::Api` delegates `embed_batch` through `Embedder` | `core/src/api_embedder.rs` tests (mock HTTP server) |
| `Embedder::from_config` routes `BuiltIn` / `OnnxFile` / `Api` correctly | `core/src/embedder.rs` tests |
| CLI warns when `api_key` is in config | `cli/src/commands/chart.rs` tests (capture stderr) |
| Stub module compiles without `semantic` | `cargo check --no-default-features` |

## Files to Create / Modify

| File | Action |
|------|--------|
| `core/src/api_embedder.rs` | **New** — `ApiClient` (internal), `ApiEmbedder` (internal), OpenAI request/response types |
| `core/src/embedder.rs` | Modify — add `EmbedderBackend` enum, add `Embedder::from_config()`, keep `Embedder` as single public type |
| `core/src/config.rs` | Modify — add `Api` variant, add `create_embedder()` |
| `core/src/lib.rs` | Modify — register `api_embedder` module, update stub module |
| `core/Cargo.toml` | Modify — add `ureq` dependency (optional, under `semantic` feature) |
| `cli/src/commands/chart.rs` | Modify — replace `resolve_model_path()` with `create_embedder()` |
| `cli/src/commands/scan.rs` | Modify — same as above |
| `cli/src/messages.rs` | Modify — add `WARN_API_KEY_IN_CONFIG` constant |

## Rollback / Compatibility

- Existing `BuiltIn` and `OnnxFile` configs are unaffected.
- `EmbedderConfig` uses `#[serde(tag = "provider")]`; omitting `provider` defaults to `BuiltIn`.
- Old `SHIOTSUCHI_EMBED_MODEL_PATH` env var continues working for `BuiltIn` resolution.
- API provider is opt-in; no migration needed.
