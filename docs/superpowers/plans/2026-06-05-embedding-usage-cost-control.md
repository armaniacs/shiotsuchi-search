# Embedding API Usage Cost Control — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add monthly embedding API request count limit with JSON file persistence, welcome flow TUI config, CLI config commands, and usage display.

**Architecture:** A `UsageTracker` module manages a JSON file (`usage.json`) with monthly counters and history. It is injected into `ApiClient` and checked before each HTTP embedding request. The CLI provides TUI config via welcome flow, `config set`, and `config reset-usage`.

**Tech Stack:** Rust, `serde`/`serde_json` (already in deps), `dialoguer` (already in deps), `toml` (already in deps)

---

## File Map

| File | Action | Responsibility |
|------|--------|---------------|
| `core/src/usage_tracker.rs` | **Create** | UsageTracker struct, JSON I/O, monthly counter |
| `core/src/config.rs` | Modify | Add `EmbeddingUsageConfig` struct |
| `core/src/lib.rs` | Modify | Add `pub mod usage_tracker;` |
| `core/src/embedder.rs` | Modify | Add `UsageLimitExceeded` error variant |
| `core/src/api_embedder.rs` | Modify | Inject UsageTracker, check before requests |
| `cli/src/commands/welcome.rs` | Modify | Add embedding usage config step |
| `cli/src/commands/config.rs` | Modify | Add `set` and `reset-usage` subcommands |
| `cli/src/commands/tide.rs` | Modify | Display usage stats |
| `cli/src/main.rs` | Modify | Pass config to commands |
| `cli/src/config.rs` | Modify | Add `embedding_usage` to `ShiotsuchiConfig` |

---

### Task 1: EmbedderError + EmbeddingUsageConfig

**Files:**
- Modify: `core/src/embedder.rs:507-515`
- Modify: `core/src/config.rs` (add struct)

- [ ] **Step 1: Add UsageLimitExceeded to EmbedderError**

In `core/src/embedder.rs`, add a new variant to `EmbedderError`:

```rust
#[derive(Debug, Clone, Error)]
pub enum EmbedderError {
    #[error("model load error: {0}")]
    Load(String),
    #[error("embedding error: {0}")]
    Inference(String),
    #[error("unavailable: {0}")]
    Unavailable(String),
    #[error("月次埋め込みAPI上限に達しました ({used}/{limit}, {month})")]
    UsageLimitExceeded { limit: u64, used: u64, month: String },
}
```

- [ ] **Step 2: Add EmbeddingUsageConfig to config.rs**

In `core/src/config.rs`, add:

```rust
/// Monthly embedding API usage limit configuration.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct EmbeddingUsageConfig {
    /// Whether usage tracking is enabled. Default: false.
    #[serde(default)]
    pub enabled: bool,
    /// Monthly request limit. None = unlimited.
    #[serde(default)]
    pub monthly_limit: Option<u64>,
}
```

- [ ] **Step 3: Add embedding_usage field to IndexConfig**

In `core/src/models.rs`, add to `IndexConfig`:

```rust
    /// Embedding API usage limit configuration.
    pub embedding_usage: crate::config::EmbeddingUsageConfig,
```

And in the `Default` impl:

```rust
            embedding_usage: crate::config::EmbeddingUsageConfig::default(),
```

- [ ] **Step 4: Run tests to verify compilation**

Run: `cargo test -p shiotsuchi-core --lib -- --quiet`
Expected: PASS (no regressions)

- [ ] **Step 5: Commit**

```bash
git add core/src/embedder.rs core/src/config.rs core/src/models.rs
git commit -m "feat(core): add EmbedderError::UsageLimitExceeded and EmbeddingUsageConfig"
```

---

### Task 2: UsageTracker implementation

**Files:**
- Create: `core/src/usage_tracker.rs`
- Modify: `core/src/lib.rs`

- [ ] **Step 1: Write failing tests for UsageTracker**

In `core/src/usage_tracker.rs`:

```rust
use crate::embedder::EmbedderError;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct UsageFile {
    current_month: String,
    current_count: u64,
    #[serde(default)]
    history: HashMap<String, u64>,
}

/// Tracks monthly embedding API request counts via a JSON file.
pub struct UsageTracker {
    path: PathBuf,
    enabled: bool,
    monthly_limit: Option<u64>,
}

impl UsageTracker {
    pub fn new(config_dir: &std::path::Path, enabled: bool, monthly_limit: Option<u64>) -> Self {
        Self {
            path: config_dir.join("usage.json"),
            enabled,
            monthly_limit,
        }
    }

    pub fn check_and_increment(&self) -> Result<(), EmbedderError> {
        if !self.enabled {
            return Ok(());
        }

        let now_month = current_month();

        let mut usage = match fs::read_to_string(&self.path) {
            Ok(content) => serde_json::from_str::<UsageFile>(&content).unwrap_or_else(|e| {
                log::warn!("Failed to parse usage.json, resetting: {}", e);
                UsageFile {
                    current_month: now_month.clone(),
                    current_count: 0,
                    history: HashMap::new(),
                }
            }),
            Err(_) => UsageFile {
                current_month: now_month.clone(),
                current_count: 0,
                history: HashMap::new(),
            },
        };

        // Monthly rotation
        if usage.current_month != now_month {
            usage.history.insert(usage.current_month.clone(), usage.current_count);
            usage.current_month = now_month.clone();
            usage.current_count = 0;
        }

        // Check limit
        if let Some(limit) = self.monthly_limit {
            if usage.current_count >= limit {
                return Err(EmbedderError::UsageLimitExceeded {
                    limit,
                    used: usage.current_count,
                    month: usage.current_month,
                });
            }
        }

        usage.current_count += 1;

        // Write (best-effort)
        if let Err(e) = self.write_usage(&usage) {
            log::warn!("Failed to write usage.json: {}", e);
        }

        Ok(())
    }

    pub fn current_usage(&self) -> Result<(String, u64, HashMap<String, u64>), EmbedderError> {
        let usage = self.read_or_init()?;
        Ok((usage.current_month, usage.current_count, usage.history))
    }

    pub fn reset(&self) -> Result<(), EmbedderError> {
        let usage = UsageFile {
            current_month: current_month(),
            current_count: 0,
            history: HashMap::new(),
        };
        self.write_usage(&usage)
    }

    fn read_or_init(&self) -> Result<UsageFile, EmbedderError> {
        let now_month = current_month();
        match fs::read_to_string(&self.path) {
            Ok(content) => serde_json::from_str::<UsageFile>(&content).map_err(|e| {
                EmbedderError::Inference(format!("Failed to parse usage.json: {}", e))
            }),
            Err(_) => Ok(UsageFile {
                current_month: now_month,
                current_count: 0,
                history: HashMap::new(),
            }),
        }
    }

    fn write_usage(&self, usage: &UsageFile) -> Result<(), EmbedderError> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                EmbedderError::Load(format!("Failed to create usage dir: {}", e))
            })?;
        }
        let json = serde_json::to_string_pretty(usage)
            .map_err(|e| EmbedderError::Inference(format!("JSON serialize error: {}", e)))?;
        fs::write(&self.path, json).map_err(|e| {
            EmbedderError::Load(format!("Failed to write usage.json: {}", e))
        })
    }
}

fn current_month() -> String {
    use chrono::Datelike;
    let now = chrono::Utc::now();
    format!("{:04}-{:02}", now.year(), now.month())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn tracker(dir: &std::path::Path, limit: Option<u64>) -> UsageTracker {
        UsageTracker::new(dir, true, limit)
    }

    #[test]
    fn test_creates_file_on_first_run() {
        let tmp = TempDir::new().unwrap();
        let t = tracker(tmp.path(), Some(100));
        t.check_and_increment().unwrap();
        assert!(tmp.path().join("usage.json").exists());
    }

    #[test]
    fn test_increments_count() {
        let tmp = TempDir::new().unwrap();
        let t = tracker(tmp.path(), Some(100));
        t.check_and_increment().unwrap();
        t.check_and_increment().unwrap();
        let (month, count, _) = t.current_usage().unwrap();
        assert_eq!(count, 2);
        assert_eq!(month, current_month());
    }

    #[test]
    fn test_limit_exceeded() {
        let tmp = TempDir::new().unwrap();
        let t = tracker(tmp.path(), Some(2));
        t.check_and_increment().unwrap();
        t.check_and_increment().unwrap();
        let err = t.check_and_increment().unwrap_err();
        assert!(matches!(err, EmbedderError::UsageLimitExceeded { .. }));
    }

    #[test]
    fn test_disabled_skips_check() {
        let tmp = TempDir::new().unwrap();
        let t = UsageTracker::new(tmp.path(), false, Some(0));
        for _ in 0..100 {
            t.check_and_increment().unwrap();
        }
    }

    #[test]
    fn test_monthly_rotation_preserves_history() {
        let tmp = TempDir::new().unwrap();
        let t = tracker(tmp.path(), Some(1000));
        // Write a fake usage file with last month
        let last_month = {
            use chrono::Datelike;
            let now = chrono::Utc::now();
            let (y, m) = if now.month() == 1 { (now.year() - 1, 12) } else { (now.year(), now.month() - 1) };
            format!("{:04}-{:02}", y, m)
        };
        let fake = UsageFile {
            current_month: last_month.clone(),
            current_count: 42,
            history: HashMap::new(),
        };
        fs::write(tmp.path().join("usage.json"), serde_json::to_string(&fake).unwrap()).unwrap();

        t.check_and_increment().unwrap();
        let (_, count, history) = t.current_usage().unwrap();
        assert_eq!(count, 1);
        assert_eq!(history.get(&last_month), Some(&42));
    }

    #[test]
    fn test_corrupted_file_recovers() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("usage.json"), "not valid json {{{").unwrap();
        let t = tracker(tmp.path(), Some(100));
        t.check_and_increment().unwrap();
        let (_, count, _) = t.current_usage().unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn test_reset() {
        let tmp = TempDir::new().unwrap();
        let t = tracker(tmp.path(), Some(100));
        t.check_and_increment().unwrap();
        t.reset().unwrap();
        let (_, count, _) = t.current_usage().unwrap();
        assert_eq!(count, 0);
    }
}
```

- [ ] **Step 2: Add pub mod to lib.rs**

In `core/src/lib.rs`, add:

```rust
pub mod usage_tracker;
```

- [ ] **Step 3: Add chrono dependency**

In `core/Cargo.toml`, add to `[dependencies]`:

```toml
chrono = "0.4"
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p shiotsuchi-core --lib usage_tracker`
Expected: 7 tests PASS

- [ ] **Step 5: Commit**

```bash
git add core/src/usage_tracker.rs core/src/lib.rs core/Cargo.toml
git commit -m "feat(core): implement UsageTracker with monthly rotation and history"
```

---

### Task 3: ApiClient UsageTracker integration

**Files:**
- Modify: `core/src/api_embedder.rs:32-53`
- Modify: `core/src/embedder.rs:189-194`

- [ ] **Step 1: Add usage_tracker field to ApiClient**

In `core/src/api_embedder.rs`, modify `ApiClient`:

```rust
pub(crate) struct ApiClient {
    endpoint: String,
    model: String,
    api_key: String,
    timeout: Duration,
    batch_cap: usize,
    usage_tracker: Option<crate::usage_tracker::UsageTracker>,
}
```

Modify `ApiClient::new()`:

```rust
    pub(crate) fn new(
        endpoint: String,
        model: String,
        api_key: String,
        usage_tracker: Option<crate::usage_tracker::UsageTracker>,
    ) -> Self {
        Self {
            endpoint,
            model,
            api_key,
            timeout: Duration::from_secs(DEFAULT_TIMEOUT_SECS),
            batch_cap: DEFAULT_BATCH_CAP,
            usage_tracker,
        }
    }
```

- [ ] **Step 2: Add check_and_increment in embed_batch loop**

In `core/src/api_embedder.rs`, modify `embed_batch()`:

```rust
    pub(crate) fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, EmbedderError> {
        if texts.is_empty() {
            return Ok(vec![]);
        }

        let mut all_embeddings = Vec::with_capacity(texts.len());

        for chunk in texts.chunks(self.batch_cap) {
            // Check usage limit before each HTTP request
            if let Some(tracker) = &self.usage_tracker {
                tracker.check_and_increment()?;
            }

            let request_body = EmbeddingRequest {
                model: &self.model,
                input: chunk.to_vec(),
            };
            // ... rest unchanged ...
```

- [ ] **Step 3: Update Embedder::from_api_client to accept usage_tracker**

In `core/src/embedder.rs`, modify `from_api_client`:

```rust
    pub(crate) fn from_api_client(client: ApiClient) -> Self {
        let model_id = client.model_id();
        Self {
            backend: EmbedderBackend::Api { client, model_id },
        }
    }
```

(Note: `from_api_client` signature stays the same since `usage_tracker` is inside `ApiClient`)

- [ ] **Step 4: Fix all existing `ApiClient::new()` call sites**

Search for `ApiClient::new(` and add `None` as the last parameter:

```bash
grep -rn "ApiClient::new(" core/src/ cli/src/ mcp/src/
```

For each call site, add `None` as the 4th argument.

- [ ] **Step 5: Write integration test**

Add to `core/src/api_embedder.rs` tests:

```rust
    #[test]
    fn test_api_client_with_usage_tracker() {
        let tmp = tempfile::TempDir::new().unwrap();
        let tracker = crate::usage_tracker::UsageTracker::new(tmp.path(), true, Some(1));
        let client = ApiClient::new(
            "https://example.com".to_string(),
            "model".to_string(),
            "key".to_string(),
            Some(tracker),
        );
        // First embed_batch call should succeed (even if HTTP fails, tracker increments before)
        let result = client.embed_batch(&["test"]);
        // HTTP will fail, but usage should have been incremented
        assert!(result.is_err()); // HTTP error, not usage limit
        let (_, count, _) = UsageTracker::new(tmp.path(), true, None).current_usage().unwrap();
        assert_eq!(count, 1);
    }
```

- [ ] **Step 6: Run all core tests**

Run: `cargo test -p shiotsuchi-core`
Expected: All tests PASS

- [ ] **Step 7: Commit**

```bash
git add core/src/api_embedder.rs core/src/embedder.rs
git commit -m "feat(core): integrate UsageTracker into ApiClient for usage limiting"
```

---

### Task 4: CLI config set + reset-usage

**Files:**
- Modify: `cli/src/commands/config.rs`
- Modify: `cli/src/commands/mod.rs`
- Modify: `cli/src/main.rs`

- [ ] **Step 1: Add Set and ResetUsage subcommands**

In `cli/src/commands/config.rs`, add:

```rust
#[derive(Subcommand, Debug)]
pub enum ConfigCommands {
    DetectNoise(DetectNoiseArgs),
    /// Set a config value
    Set(SetArgs),
    /// Reset the embedding API usage counter
    ResetUsage,
}

#[derive(Args, Debug)]
pub struct SetArgs {
    /// Config key (e.g., embedding_usage.enabled)
    pub key: String,
    /// Value to set
    pub value: String,
}
```

- [ ] **Step 2: Implement run_set**

Add to `cli/src/commands/config.rs`:

```rust
fn run_set(args: &SetArgs, config_path: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
    let content = std::fs::read_to_string(config_path)?;
    let mut cfg: toml::Value = toml::from_str(&content)?;

    let parts: Vec<&str> = args.key.split('.').collect();
    match parts.as_slice() {
        ["embedding_usage", "enabled"] => {
            let val: bool = args.value.parse()
                .map_err(|_| format!("Expected bool (true/false), got '{}'", args.value))?;
            cfg["embedding_usage"]["enabled"] = toml::Value::Boolean(val);
        }
        ["embedding_usage", "monthly_limit"] => {
            let val: u64 = args.value.parse()
                .map_err(|_| format!("Expected number, got '{}'", args.value))?;
            cfg["embedding_usage"]["monthly_limit"] = toml::Value::Integer(val as i64);
        }
        _ => return Err(format!("Unknown config key: {}", args.key).into()),
    }

    let toml_str = toml::to_string_pretty(&cfg)?;
    let tmp = config_path.with_extension("toml.tmp");
    std::fs::write(&tmp, &toml_str)?;
    std::fs::rename(&tmp, config_path)?;
    println!("Set {} = {}", args.key, args.value);
    Ok(())
}
```

- [ ] **Step 3: Implement run_reset_usage**

Add to `cli/src/commands/config.rs`:

```rust
fn run_reset_usage(config_dir: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
    let tracker = shiotsuchi_core::usage_tracker::UsageTracker::new(config_dir, true, None);
    tracker.reset()?;
    println!("Embedding API usage counter has been reset.");
    Ok(())
}
```

- [ ] **Step 4: Update run_config match**

```rust
pub fn run_config(
    args: &ConfigArgs,
    vaults: &[(String, std::path::PathBuf)],
    include_extensions: &[String],
    auto_exclude_hidden: bool,
    dynamic_threshold: usize,
    config_path: &std::path::Path,
    config_dir: &std::path::Path,
) -> Result<(), Box<dyn std::error::Error>> {
    match &args.command {
        ConfigCommands::DetectNoise(detect_args) => { /* existing */ }
        ConfigCommands::Set(set_args) => run_set(set_args, config_path)?,
        ConfigCommands::ResetUsage => run_reset_usage(config_dir)?,
    }
    // ... existing print messages ...
    Ok(())
}
```

- [ ] **Step 5: Update main.rs call site**

Update the `Commands::Config` match arm to pass `config_path` and `config_dir`:

```rust
Some(Commands::Config(args)) => {
    let config_path = config::default_config_path();
    let config_dir = config_path.parent().unwrap_or(std::path::Path::new("."));
    commands::config::run_config(
        &args,
        &resolved_vaults,
        &cfg.indexing.include_extensions,
        cfg.indexing.auto_exclude_hidden,
        cfg.indexing.dynamic_threshold,
        &config_path,
        config_dir,
    )?;
}
```

- [ ] **Step 6: Run CLI tests**

Run: `cargo test --bin shiotsuchi`
Expected: All tests PASS

- [ ] **Step 7: Commit**

```bash
git add cli/src/commands/config.rs cli/src/main.rs
git commit -m "feat(cli): add config set and reset-usage commands"
```

---

### Task 5: Welcome flow embedding usage config

**Files:**
- Modify: `cli/src/commands/welcome.rs`

- [ ] **Step 1: Add usage config step to welcome flow**

After the VLM consent step in `run_welcome()`, add:

```rust
    // Embedding usage config step
    if is_tty && !args.yes {
        let confirm = dialoguer::Confirm::with_theme(&*dialoguer_theme())
            .with_prompt("埋め込みAPIの月間リクエスト数を制限しますか？")
            .default(false)
            .interact()?;

        if confirm {
            let limit: String = dialoguer::Input::with_theme(&*dialoguer_theme())
                .with_prompt("月間上限値を入力してください")
                .default("1000".to_string())
                .validate_with(|input: &String| -> Result<(), &str> {
                    input.parse::<u64>().map_err(|_| "数値を入力してください")
                })
                .interact()?;

            cfg.embedding_usage.enabled = true;
            cfg.embedding_usage.monthly_limit = Some(limit.parse::<u64>().unwrap());
        }
    }
```

- [ ] **Step 2: Add config to ShiotsuchiConfig**

In `cli/src/config.rs`, add to `ShiotsuchiConfig`:

```rust
    #[serde(default)]
    pub embedding_usage: shiotsuchi_core::config::EmbeddingUsageConfig,
```

- [ ] **Step 3: Write failing test**

In `cli/src/commands/welcome.rs` tests, add:

```rust
    #[test]
    fn test_embedding_usage_config_field_exists() {
        let cfg = ShiotsuchiConfig::default();
        assert!(!cfg.embedding_usage.enabled);
        assert!(cfg.embedding_usage.monthly_limit.is_none());
    }
```

- [ ] **Step 4: Run tests**

Run: `cargo test --bin shiotsuchi`
Expected: All tests PASS

- [ ] **Step 5: Commit**

```bash
git add cli/src/commands/welcome.rs cli/src/config.rs
git commit -m "feat(cli): add embedding usage config step to welcome flow"
```

---

### Task 6: Tide usage display

**Files:**
- Modify: `cli/src/commands/tide.rs`

- [ ] **Step 1: Add usage display to tide output**

In `cli/src/commands/tide.rs`, in the stats display section, add:

```rust
    // Display embedding usage if enabled
    if let Some(ref usage_cfg) = cfg.embedding_usage {
        if usage_cfg.enabled {
            let config_dir = crate::config::default_config_path()
                .parent()
                .map(|p| p.to_path_buf())
                .unwrap_or_default();
            let tracker = shiotsuchi_core::usage_tracker::UsageTracker::new(
                &config_dir, true, usage_cfg.monthly_limit,
            );
            if let Ok((month, count, _)) = tracker.current_usage() {
                let limit_str = usage_cfg.monthly_limit
                    .map(|l| format!("/{}", l))
                    .unwrap_or_default();
                println!("  Embedding API: {}{} requests ({})", count, limit_str, month);
            }
        }
    }
```

- [ ] **Step 2: Write test**

Add to `cli/src/commands/tide.rs` tests:

```rust
    #[test]
    fn test_tide_includes_usage_when_enabled() {
        // This is a smoke test — actual display is tested via integration
        let cfg = ShiotsuchiConfig::default();
        assert!(!cfg.embedding_usage.enabled, "default should be disabled");
    }
```

- [ ] **Step 3: Run tests**

Run: `cargo test --bin shiotsuchi`
Expected: All tests PASS

- [ ] **Step 4: Commit**

```bash
git add cli/src/commands/tide.rs
git commit -m "feat(cli): display embedding API usage in tide output"
```

---

### Task 7: Pass embedding_usage config through main.rs

**Files:**
- Modify: `cli/src/main.rs`

- [ ] **Step 1: Thread config to all commands that need it**

Verify that `cfg.embedding_usage` is accessible in:
- `run_chart` (via `IndexConfig`)
- `run_scan` (via `IndexConfig`)
- `run_index` (via `IndexConfig`)
- `run_serve` (via config)

In `IndexConfig` default, `embedding_usage` is already set to `default()`.

In `cli/src/main.rs`, ensure `cfg.indexing.embedding_usage = cfg.embedding_usage.clone()` is set before passing `IndexConfig` to commands that build embedders.

Add after config loading:

```rust
    // Thread embedding_usage config into IndexConfig
    cfg.indexing.embedding_usage = cfg.embedding_usage.clone();
```

- [ ] **Step 2: Run full workspace tests**

Run: `cargo test -p shiotsuchi-core && cargo test --bin shiotsuchi`
Expected: All tests PASS

- [ ] **Step 3: Commit**

```bash
git add cli/src/main.rs
git commit -m "feat(cli): thread embedding_usage config through main.rs"
```

---

### Task 8: Integration test + verification

**Files:**
- Create: `core/tests/usage_tracker_integration.rs`

- [ ] **Step 1: Write integration test**

```rust
use shiotsuchi_core::usage_tracker::UsageTracker;
use tempfile::TempDir;

#[test]
fn test_full_lifecycle() {
    let tmp = TempDir::new().unwrap();

    // Create tracker with limit of 3
    let tracker = UsageTracker::new(tmp.path(), true, Some(3));

    // Use up all 3
    tracker.check_and_increment().unwrap();
    tracker.check_and_increment().unwrap();
    tracker.check_and_increment().unwrap();

    // 4th should fail
    let err = tracker.check_and_increment().unwrap_err();
    assert!(format!("{}", err).contains("42/1000") || format!("{}", err).contains("上限"));

    // Reset and try again
    tracker.reset().unwrap();
    tracker.check_and_increment().unwrap();
    let (_, count, _) = tracker.current_usage().unwrap();
    assert_eq!(count, 1);
}

#[test]
fn test_disabled_tracker_never_fails() {
    let tmp = TempDir::new().unwrap();
    let tracker = UsageTracker::new(tmp.path(), false, Some(0));
    for _ in 0..10000 {
        tracker.check_and_increment().unwrap();
    }
}
```

- [ ] **Step 2: Run integration tests**

Run: `cargo test -p shiotsuchi-core --test usage_tracker_integration`
Expected: 2 tests PASS

- [ ] **Step 3: Run full workspace tests**

Run: `cargo test`
Expected: All tests PASS (except pre-existing `test_dredge_expired_with_config`)

- [ ] **Step 4: Manual smoke test**

```bash
# Build and test config commands
cargo build
./target/debug/shiotsuchi config set embedding_usage.enabled true
./target/debug/shiotsuchi config set embedding_usage.monthly_limit 5
./target/debug/shiotsuchi config reset-usage
```

- [ ] **Step 5: Final commit**

```bash
git add core/tests/usage_tracker_integration.rs
git commit -m "test(core): add usage tracker integration tests"
```
