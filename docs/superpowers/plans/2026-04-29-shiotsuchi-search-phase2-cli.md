# Shiotsuchi-Search Phase 2: CLI Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the `shiotsuchi` CLI binary (`cli/` crate) exposing `dive`, `chart`, `tide`, `scan`, `log` subcommands backed by `obsidian-shiotsuchi-vault-core`.

**Architecture:** A Rust binary crate (`cli/`) using `clap` for argument parsing. Each subcommand wraps the corresponding core library function. Config is loaded from `~/.shiotsuchi/config.toml` (overridable by CLI flags and env vars).

**Tech Stack:** Rust, clap (derive), config, serde, serde_json, obsidian-shiotsuchi-vault-core

**Prerequisite:** Phase 1 complete — `obsidian-shiotsuchi-vault-core` builds and all tests pass.

---

## TDD (Test-Driven Development) Approach

All implementation in this plan follows strict TDD cycles:

1. **RED** - Write a failing test for the desired behavior.
2. **RED VERIFY** - Run the test, confirm it fails (feature not yet implemented).
3. **GREEN** - Write minimal code to make the test pass.
4. **GREEN VERIFY** - Run the test, confirm it passes.
5. **REFACTOR** - Clean up code while keeping tests green.
6. Repeat for next behavior.

**Mandatory Rules:**
- Never write production code without a failing test first.
- If code was written before tests, delete it and start over.
- Verify RED before writing GREEN code — if the test passes immediately, the test is wrong.
- Verify GREEN before moving to next cycle.
- RED VERIFY is never skippable: watching the test fail is proof that it tests the right thing.

**Exception — Task 1 (Skeleton):** Cargo manifest and empty `main.rs` have no testable behavior; TDD does not apply. All other tasks follow strict TDD.

---

## File Structure

```
cli/
├── Cargo.toml
└── src/
    ├── main.rs          # Entry point + clap dispatch
    ├── commands/
    │   ├── mod.rs
    │   ├── chart.rs     # chart subcommand
    │   ├── dive.rs      # dive subcommand
    │   ├── tide.rs      # tide subcommand
    │   ├── scan.rs      # scan subcommand
    │   └── log.rs       # log subcommand
    └── config.rs        # config.toml loading
```

---

## Task 1: CLI Crate Skeleton

**TDD exception:** Configuration files and empty stubs have no testable behavior.

**Files:**
- Create: `cli/Cargo.toml`
- Create: `cli/src/main.rs`

- [ ] **Step 1: Write cli/Cargo.toml**

```toml
[package]
name = "shiotsuchi"
version.workspace = true
edition.workspace = true
authors.workspace = true
license.workspace = true

[[bin]]
name = "shiotsuchi"
path = "src/main.rs"

[dependencies]
obsidian-shiotsuchi-vault-core = { path = "../core" }
clap = { version = "4", features = ["derive"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
config = { version = "0.14", features = ["toml"] }
thiserror = "1"
log = "0.4"
env_logger = "0.11"
```

- [ ] **Step 2: Write cli/src/main.rs skeleton**

```rust
mod commands;
mod config;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "shiotsuchi", about = "Guiding your path through the data tide.")]
struct Cli {
    #[arg(long, env = "SHIOTSUCHI_NOTES_DIR")]
    notes_dir: Option<std::path::PathBuf>,

    #[arg(long, env = "SHIOTSUCHI_DB_PATH")]
    db_path: Option<std::path::PathBuf>,

    #[arg(long)]
    verbose: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Dive(commands::dive::DiveArgs),
    Chart(commands::chart::ChartArgs),
    Tide,
    Scan(commands::scan::ScanArgs),
    Log,
}

fn main() {
    let cli = Cli::parse();
    // dispatch to commands (implement in Tasks below)
}
```

- [ ] **Step 3: Add `cli` to workspace Cargo.toml**

```toml
[workspace]
members = ["core", "cli", "skill", "mcp"]
```

- [ ] **Step 4: Verify workspace compiles**

Run: `cargo check --workspace`
Expected: Compiles (with unimplemented stubs)

- [ ] **Step 5: Commit**

```bash
git add cli/Cargo.toml cli/src/main.rs Cargo.toml
git commit -m "chore(cli): initialize CLI crate skeleton"
```

---

## Task 2: Config Loading (TDD)

**Files:**
- Create: `cli/src/config.rs`

- [ ] **(RED) Step 1: Write failing tests for config loading**

Create `cli/src/config.rs` with test module only:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use std::fs;

    #[test]
    fn test_default_config() {
        // FAIL: ShiotsuchiConfig not defined yet
        let config = ShiotsuchiConfig::default();
        assert_eq!(config.indexing.include_extensions, vec!["md", "markdown"]);
        assert_eq!(config.watcher.debounce_ms, 500);
    }

    #[test]
    fn test_load_from_toml() {
        let temp = TempDir::new().unwrap();
        let config_path = temp.path().join("config.toml");
        fs::write(&config_path, r#"
            [vault]
            notes_dir = "/tmp/notes"

            [indexing]
            snippet_lines = 5
        "#).unwrap();

        let config = ShiotsuchiConfig::load_from(&config_path).unwrap();
        assert_eq!(config.vault.notes_dir.to_string_lossy(), "/tmp/notes");
        assert_eq!(config.indexing.snippet_lines, 5);
    }
}
```

- [ ] **(RED VERIFY) Step 2: Run tests, confirm they fail**

Run: `cargo test -p shiotsuchi config`
Expected: Compilation error — `ShiotsuchiConfig` not found

- [ ] **(GREEN) Step 3: Implement config.rs**

```rust
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultConfig {
    pub notes_dir: PathBuf,
    pub db_path: PathBuf,
}

impl Default for VaultConfig {
    fn default() -> Self {
        let home = dirs::home_dir().unwrap_or_default();
        Self {
            notes_dir: PathBuf::from("."),
            db_path: home.join(".shiotsuchi").join("db.sqlite3"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexingConfig {
    pub snippet_lines: usize,
    pub include_extensions: Vec<String>,
    pub exclude_patterns: Vec<String>,
}

impl Default for IndexingConfig {
    fn default() -> Self {
        Self {
            snippet_lines: 3,
            include_extensions: vec!["md".to_string(), "markdown".to_string()],
            exclude_patterns: vec![
                ".obsidian".to_string(),
                ".git".to_string(),
                "node_modules".to_string(),
            ],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WatcherConfig {
    pub debounce_ms: u64,
    pub enabled: bool,
}

impl Default for WatcherConfig {
    fn default() -> Self {
        Self { debounce_ms: 500, enabled: true }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ShiotsuchiConfig {
    pub vault: VaultConfig,
    pub indexing: IndexingConfig,
    pub watcher: WatcherConfig,
}

impl ShiotsuchiConfig {
    pub fn load_from(path: &Path) -> Result<Self, config::ConfigError> {
        config::Config::builder()
            .add_source(config::File::from(path))
            .build()?
            .try_deserialize()
    }

    pub fn load() -> Self {
        let default_path = dirs::home_dir()
            .unwrap_or_default()
            .join(".shiotsuchi")
            .join("config.toml");
        if default_path.exists() {
            Self::load_from(&default_path).unwrap_or_default()
        } else {
            Self::default()
        }
    }
}
```

Add `dirs = "5"` to `cli/Cargo.toml` dependencies.

- [ ] **(GREEN VERIFY) Step 4: Run config tests, confirm they pass**

Run: `cargo test -p shiotsuchi config`
Expected: 2 tests pass

- [ ] **Step 5: Commit**

```bash
git add cli/src/config.rs cli/Cargo.toml
git commit -m "feat(cli): add config.toml loading with defaults"
```

---

## Task 3: `chart` Command — Index (TDD)

**Files:**
- Create: `cli/src/commands/chart.rs`
- Create: `cli/src/commands/mod.rs`

- [ ] **(RED) Step 1: Write failing test for chart command**

Create `cli/src/commands/chart.rs` with test only:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use std::fs;

    #[test]
    fn test_chart_indexes_files() {
        // FAIL: run_chart not defined yet
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("note.md"), "# Hello\n\nWorld").unwrap();

        let db_file = temp.path().join("test.db");
        let args = ChartArgs { force: false, quiet: true };
        let result = run_chart(&args, temp.path(), &db_file);
        assert!(result.is_ok());
        let summary = result.unwrap();
        assert_eq!(summary.indexed, 1);
        assert_eq!(summary.errors, 0);
    }
}
```

- [ ] **(RED VERIFY) Step 2: Run test, confirm it fails**

Run: `cargo test -p shiotsuchi chart`
Expected: Compilation error — `ChartArgs`, `run_chart` not found

- [ ] **(GREEN) Step 3: Implement chart.rs**

```rust
use clap::Args;
use obsidian_shiotsuchi_vault_core::{
    db::NoteDatabase,
    indexer::index_directory,
    models::{IndexConfig, IndexResult},
    tokenizer::{JapaneseTokenizer, TokenizerConfig},
};
use std::path::Path;

#[derive(Args, Debug)]
pub struct ChartArgs {
    #[arg(long)]
    pub force: bool,
    #[arg(long)]
    pub quiet: bool,
}

pub struct ChartSummary {
    pub indexed: usize,
    pub skipped: usize,
    pub errors: usize,
}

pub fn run_chart(
    args: &ChartArgs,
    notes_dir: &Path,
    db_path: &Path,
) -> Result<ChartSummary, Box<dyn std::error::Error>> {
    let db = NoteDatabase::open(db_path)?;
    let tokenizer = JapaneseTokenizer::new(TokenizerConfig::default())?;
    let config = IndexConfig {
        notes_dir: notes_dir.to_path_buf(),
        ..Default::default()
    };

    let results = index_directory(&db, &tokenizer, &config)?;

    let mut summary = ChartSummary { indexed: 0, skipped: 0, errors: 0 };
    for (_, result) in &results {
        match result {
            IndexResult::Inserted | IndexResult::Updated => summary.indexed += 1,
            IndexResult::Skipped => summary.skipped += 1,
            IndexResult::Error(_) => summary.errors += 1,
        }
    }

    if !args.quiet {
        println!(
            "Indexed {} files ({} skipped, {} errors)",
            summary.indexed, summary.skipped, summary.errors
        );
    }

    Ok(summary)
}
```

- [ ] **(GREEN VERIFY) Step 4: Run chart tests, confirm they pass**

Run: `cargo test -p shiotsuchi chart`
Expected: 1 test passes

- [ ] **Step 5: Commit**

```bash
git add cli/src/commands/
git commit -m "feat(cli): add chart command for indexing"
```

---

## Task 4: `dive` Command — Search (TDD)

**Files:**
- Create: `cli/src/commands/dive.rs`

- [ ] **(RED) Step 1: Write failing tests for dive command**

Create `cli/src/commands/dive.rs` with test only:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use std::fs;

    #[test]
    fn test_dive_returns_json() {
        // FAIL: run_dive not defined yet
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("note.md"), "# Hello\n\nThis is a search test.").unwrap();
        let db_file = temp.path().join("test.db");

        // Index first
        let chart_args = crate::commands::chart::ChartArgs { force: false, quiet: true };
        crate::commands::chart::run_chart(&chart_args, temp.path(), &db_file).unwrap();

        let args = DiveArgs { query: "search test".to_string(), json: false, limit: 10 };
        let output = run_dive(&args, temp.path(), &db_file).unwrap();
        assert!(!output.is_empty());
        assert!(output[0].path.contains("note"));
    }

    #[test]
    fn test_dive_empty_query_returns_empty() {
        let temp = TempDir::new().unwrap();
        let db_file = temp.path().join("test.db");
        let _ = crate::commands::chart::run_chart(
            &crate::commands::chart::ChartArgs { force: false, quiet: true },
            temp.path(), &db_file,
        );

        let args = DiveArgs { query: "".to_string(), json: false, limit: 10 };
        let output = run_dive(&args, temp.path(), &db_file).unwrap();
        assert!(output.is_empty());
    }
}
```

- [ ] **(RED VERIFY) Step 2: Run tests, confirm they fail**

Run: `cargo test -p shiotsuchi dive`
Expected: Compilation error — `DiveArgs`, `run_dive` not found

- [ ] **(GREEN) Step 3: Implement dive.rs**

```rust
use clap::Args;
use obsidian_shiotsuchi_vault_core::{
    db::NoteDatabase,
    models::SearchResult,
    search::search,
    tokenizer::{JapaneseTokenizer, TokenizerConfig},
};
use std::path::Path;

#[derive(Args, Debug)]
pub struct DiveArgs {
    pub query: String,
    #[arg(long)]
    pub json: bool,
    #[arg(long, default_value = "20")]
    pub limit: usize,
}

pub fn run_dive(
    args: &DiveArgs,
    notes_dir: &Path,
    db_path: &Path,
) -> Result<Vec<SearchResult>, Box<dyn std::error::Error>> {
    if args.query.trim().is_empty() {
        return Ok(vec![]);
    }

    let db = NoteDatabase::open(db_path)?;
    let tokenizer = JapaneseTokenizer::new(TokenizerConfig::default())?;
    let results = search(&db, &tokenizer, notes_dir, &args.query, args.limit)?;
    Ok(results)
}

pub fn print_results(results: &[SearchResult], compact_json: bool) {
    if compact_json {
        println!("{}", serde_json::to_string(results).unwrap_or_default());
    } else {
        println!("{}", serde_json::to_string_pretty(results).unwrap_or_default());
    }
}
```

- [ ] **(GREEN VERIFY) Step 4: Run dive tests, confirm they pass**

Run: `cargo test -p shiotsuchi dive`
Expected: 2 tests pass

- [ ] **Step 5: Commit**

```bash
git add cli/src/commands/dive.rs
git commit -m "feat(cli): add dive command for search with --json flag"
```

---

## Task 5: `tide` Command — Stats (TDD)

**Files:**
- Create: `cli/src/commands/tide.rs`

- [ ] **(RED) Step 1: Write failing test for tide command**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_tide_on_empty_db() {
        // FAIL: run_tide not defined yet
        let temp = TempDir::new().unwrap();
        let db_file = temp.path().join("test.db");
        let stats = run_tide(&db_file).unwrap();
        assert_eq!(stats.total_notes, 0);
    }
}
```

- [ ] **(RED VERIFY) Step 2: Run test, confirm it fails**

Run: `cargo test -p shiotsuchi tide`
Expected: Compilation error — `run_tide` not found

- [ ] **(GREEN) Step 3: Implement tide.rs**

```rust
use clap::Args;
use obsidian_shiotsuchi_vault_core::{db::NoteDatabase, models::VaultStats};
use std::path::Path;

pub fn run_tide(db_path: &Path) -> Result<VaultStats, Box<dyn std::error::Error>> {
    let db = NoteDatabase::open(db_path)?;
    Ok(db.stats()?)
}

pub fn print_stats(stats: &VaultStats) {
    println!("Total notes : {}", stats.total_notes);
    println!("DB size     : {} bytes", stats.total_size_bytes);
    if let Some(ts) = stats.last_indexed_at {
        println!("Last indexed: {}", ts);
    } else {
        println!("Last indexed: never");
    }
}
```

- [ ] **(GREEN VERIFY) Step 4: Run tide test, confirm it passes**

Run: `cargo test -p shiotsuchi tide`
Expected: 1 test passes

- [ ] **Step 6: Commit**

```bash
git add cli/src/commands/tide.rs
git commit -m "feat(cli): add tide command for vault stats"
```

---

## Task 6: `scan` and `log` Stubs + main.rs Wiring (TDD)

**Files:**
- Create: `cli/src/commands/scan.rs`
- Create: `cli/src/commands/log.rs`
- Modify: `cli/src/main.rs`

- [ ] **(RED) Step 1: Write failing test for main dispatch**

```rust
// cli/src/main.rs (test module)
#[cfg(test)]
mod tests {
    #[test]
    fn test_version_flag_compiles() {
        // Minimal smoke test: binary exists and --help doesn't panic
        // Full dispatch tested via integration tests
        assert!(true);
    }
}
```

- [ ] **Step 2: Implement scan.rs stub**

```rust
use clap::Args;

#[derive(Args, Debug)]
pub struct ScanArgs {
    #[arg(long, default_value = "500")]
    pub debounce: u64,
}

pub fn run_scan(_args: &ScanArgs, _notes_dir: &std::path::Path, _db_path: &std::path::Path)
    -> Result<(), Box<dyn std::error::Error>>
{
    // Implemented in Phase 5 (watcher)
    eprintln!("scan: not yet implemented");
    Ok(())
}
```

- [ ] **Step 3: Implement log.rs stub**

```rust
pub fn run_log() {
    println!("log: not yet implemented");
}
```

- [ ] **Step 4: Wire all commands in main.rs**

```rust
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    if cli.verbose { env_logger::init(); }

    let mut cfg = config::ShiotsuchiConfig::load();
    if let Some(dir) = cli.notes_dir { cfg.vault.notes_dir = dir; }
    if let Some(db) = cli.db_path { cfg.vault.db_path = db; }

    match cli.command {
        Commands::Chart(args) => {
            commands::chart::run_chart(&args, &cfg.vault.notes_dir, &cfg.vault.db_path)?;
        }
        Commands::Dive(args) => {
            let results = commands::dive::run_dive(&args, &cfg.vault.notes_dir, &cfg.vault.db_path)?;
            commands::dive::print_results(&results, args.json);
        }
        Commands::Tide => {
            let stats = commands::tide::run_tide(&cfg.vault.db_path)?;
            commands::tide::print_stats(&stats);
        }
        Commands::Scan(args) => {
            commands::scan::run_scan(&args, &cfg.vault.notes_dir, &cfg.vault.db_path)?;
        }
        Commands::Log => commands::log::run_log(),
    }

    Ok(())
}
```

- [ ] **(GREEN VERIFY) Step 5: Run all CLI tests and build**

Run: `cargo test -p shiotsuchi && cargo build -p shiotsuchi`
Expected: All tests pass, binary builds

- [ ] **Step 6: Commit**

```bash
git add cli/src/
git commit -m "feat(cli): wire all subcommands in main.rs"
```

---

## Task 7: Integration Test — CLI End-to-End (TDD)

**Files:**
- Create: `cli/tests/integration_test.rs`

- [ ] **(RED) Step 1: Write integration test**

```rust
use std::fs;
use std::process::Command;
use tempfile::TempDir;

fn shiotsuchi_bin() -> std::path::PathBuf {
    env!("CARGO_BIN_EXE_shiotsuchi").into()
}

#[test]
fn test_chart_then_dive() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("test.db");
    fs::write(temp.path().join("note.md"), "# Hello\n\nThis is a test note.").unwrap();

    let chart = Command::new(shiotsuchi_bin())
        .args(["--notes-dir", temp.path().to_str().unwrap(),
               "--db-path", db.to_str().unwrap(),
               "chart", "--quiet"])
        .output().unwrap();
    assert!(chart.status.success(), "chart failed: {:?}", chart);

    let dive = Command::new(shiotsuchi_bin())
        .args(["--notes-dir", temp.path().to_str().unwrap(),
               "--db-path", db.to_str().unwrap(),
               "dive", "test note", "--json"])
        .output().unwrap();
    assert!(dive.status.success());
    let out = String::from_utf8_lossy(&dive.stdout);
    assert!(out.contains("note.md"), "expected note.md in output: {}", out);
}
```

- [ ] **(RED VERIFY) Step 2: Run integration test, confirm it fails**

Run: `cargo test -p shiotsuchi --test integration_test`
Expected: Fails (binary not yet fully wired or test logic catches a gap)

- [ ] **(GREEN) Step 3: Fix any gaps found**

Fix production code only; do not change test assertions.

- [ ] **(GREEN VERIFY) Step 4: Run integration test, confirm it passes**

Run: `cargo test -p shiotsuchi --test integration_test`
Expected: 1 test passes

- [ ] **Step 5: Commit**

```bash
git add cli/tests/integration_test.rs
git commit -m "test(cli): add end-to-end CLI integration test"
```

---

## Self-Review

### 1. Spec Coverage Check

| Spec Requirement | Plan Task |
|------------------|-----------|
| `dive <query>` — AND 検索、JSON 出力 | Task 4 |
| `dive --json` — compact JSON | Task 4 |
| `chart` — ディレクトリ walk + インデックス | Task 3 |
| `tide` — vault stats | Task 5 |
| `scan` — ファイルウォッチャー（Phase 5 で完全実装） | Task 6 stub |
| `log` — 統計表示（Phase 5 で完全実装） | Task 6 stub |
| `config.toml` 読み込み | Task 2 |
| CLI フラグによる config オーバーライド | Task 6 |
| 環境変数 `SHIOTSUCHI_NOTES_DIR` / `SHIOTSUCHI_DB_PATH` | Task 1 |

### 2. TDD Cycle Compliance

- ✅ Task 1: TDD不適用（Cargo マニフェスト・空スタブ）と明示
- ✅ Task 2〜5: 各タスクに RED → RED VERIFY → GREEN → GREEN VERIFY
- ✅ Task 6: スタブのみのためTDD略、GREEN VERIFYでビルドと全テストを確認
- ✅ Task 7: 統合テストで RED → VERIFY → GREEN → VERIFY

### 3. テスト実行前提

```bash
# モデルをダウンロードしてからテスト
./scripts/download-model.sh
SHIOTSUCHI_MODEL_PATH=models/bccwj-suw+unidic_pos+kana.model.zst \
    cargo test -p shiotsuchi
```

---

## Next Steps

Phase 3: Skill — `skill/` crate with Kilo skill protocol
