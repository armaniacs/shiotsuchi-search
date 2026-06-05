# Embedding API Usage Cost Control — Design Spec

**Date:** 2026-06-05
**Status:** Approved
**PBI:** PBI-54 (partial — cost limit portion)
**Scope:** Monthly embedding API request count limit with JSON file persistence

---

## Summary

Embedding API の月間リクエスト数上限を設定し、超過時にインデックス処理を中断する。設定は `~/.config/shiotsuchi/usage.json` に月次履歴付きで保存し、ウェルカムフロー / CLI / 手動 TOML 編集の3経路で設定可能にする。

## Requirements

| # | 要件 | 優先度 |
|---|------|--------|
| R1 | 月次リクエスト数のカウントと上限チェック | HIGH |
| R2 | `~/.config/shiotsuchi/usage.json` に月次履歴付きで保存 | HIGH |
| R3 | 上限到達時に `EmbedderError::UsageLimitExceeded` を返し index 処理を中断 | HIGH |
| R4 | `enabled` フィールドで有効/無効を切り替え可能（デフォルト: 無効） | HIGH |
| R5 | ウェルカムフロー（`shiotsuchi` サブコマンドなし）で TUI 設定 | MEDIUM |
| R6 | `shiotsuchi config set` で TOML ファイルを直接編集 | MEDIUM |
| R7 | `shiotsuchi tide` で現在の使用量を表示 | MEDIUM |
| R8 | `shiotsuchi config reset-usage` でカウンターをリセット | LOW |
| R9 | 過去の月の実績を履歴として保持 | MEDIUM |
| R10 | ONNX ローカル推論には影響しない（API バックエンドのみ） | HIGH |

## Out of Scope

- トークン数ベースのコスト計算
- ドル金額ベースのコスト計算
- リアルタイムダッシュボード
- API プロバイダー固有の料金体系との連携

## Architecture

### Data Flow

```
shiotsuchi index
  → IndexConfig.embedding_usage (enabled=true)
  → Embedder::embed / embed_batch
    → EmbedderBackend::Api → ApiClient::embed_batch
      → UsageTracker::check_and_increment()
        ├─ OK → HTTP リクエスト実行 → カウンター +1
        └─ OVER → Err(EmbedderError::UsageLimitExceeded)
  → エラーが伝播 → index 処理中断
```

### Module Changes

| ファイル | 変更種別 | 内容 |
|---------|---------|------|
| `core/src/usage_tracker.rs` | **新規** | `UsageTracker` struct, JSON ファイル I/O, 月次カウンター管理 |
| `core/src/config.rs` | 変更 | `EmbeddingUsageConfig` struct 追加 |
| `core/src/lib.rs` | 変更 | `pub mod usage_tracker;` 追加 |
| `core/src/api_embedder.rs` | 変更 | `ApiClient` に `UsageTracker` 注入、`embed_batch` 内でチェック |
| `core/src/embedder.rs` | 変更 | `EmbedderError::UsageLimitExceeded` variant 追加 |
| `cli/src/commands/welcome.rs` | 変更 | ウェルカムフローに embedding usage 設定ステップ追加 |
| `cli/src/commands/config.rs` | 変更 | `set` サブコマンド追加、`reset-usage` サブコマンド追加 |
| `cli/src/commands/tide.rs` | 変更 | 利用実績表示を追加 |
| `cli/src/main.rs` | 変更 | `EmbeddingUsageConfig` を各コマンドに渡す |

### Config Format

`~/.config/shiotsuchi/config.toml`:

```toml
[embedding_usage]
enabled = false
monthly_limit = 1000
```

### Usage File Format

`~/.config/shiotsuchi/usage.json`:

```json
{
  "current_month": "2026-06",
  "current_count": 42,
  "history": {
    "2026-05": 850,
    "2026-04": 312
  }
}
```

- `current_month`: 今月の `YYYY-MM`
- `current_count`: 今月のリクエスト数
- `history`: 過去の月の完了実績（月次ローテーション時に自動的に移動）

## Detailed Design

### UsageTracker (`core/src/usage_tracker.rs`)

```rust
use std::collections::HashMap;
use std::path::PathBuf;
use crate::embedder::EmbedderError;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct UsageFile {
    current_month: String,
    current_count: u64,
    #[serde(default)]
    history: HashMap<String, u64>,
}

pub struct UsageTracker {
    path: PathBuf,
    enabled: bool,
    monthly_limit: Option<u64>,
}
```

#### Methods

**`new(config_dir, enabled, monthly_limit) -> Self`**
- `path` = `config_dir.join("usage.json")`

**`check_and_increment() -> Result<(), EmbedderError>`**
1. ファイル読み込み。存在しないなら `UsageFile { current_month: now, current_count: 0, history: {} }` を作成
2. `current_month` と `YYYY-MM` を比較
3. 異なる月なら: `history[旧月] = current_count`、新月にリセット
4. `monthly_limit` が `Some(limit)` かつ `current_count >= limit` なら `Err(UsageLimitExceeded)`
5. `current_count += 1` してファイルを書き込み
6. 書き込み失敗時はログ警告のみ（過剰制限を避ける）

**`current_usage() -> Result<(String, u64, HashMap<String, u64>), EmbedderError>`**
- 現在の月、使用量、履歴を返す

**`reset() -> Result<(), EmbedderError>`**
- ファイルを削除して再初期化

### EmbedderError Extension

```rust
pub enum EmbedderError {
    Load(String),
    Inference(String),
    Unavailable(String),
    UsageLimitExceeded { limit: u64, used: u64, month: String },
}
```

Display: `"月次埋め込みAPI上限に達しました ({used}/{limit}, {month})"`

### ApiClient Integration

`ApiClient` に `usage_tracker: Option<UsageTracker>` をフィールドとして追加。

`embed_batch()` 内の各 chunk ループ（= 1 HTTP リクエスト）前に:
```rust
for chunk in texts.chunks(self.batch_cap) {
    if let Some(tracker) = &self.usage_tracker {
        tracker.check_and_increment()?;  // 1 HTTP リクエスト = 1 カウント
    }
    // ... existing HTTP request logic ...
}
```

**カウント粒度**: 1 HTTP リクエスト = 1 カウント。batch_cap（100件）で分割された各 chunk が1リクエストに相当。

`ApiClient::new()` に `usage_tracker: Option<UsageTracker>` パラメータを追加。

### CLI Integration

#### Welcome Flow (`cli/src/commands/welcome.rs`)

VLM 同意ステップの後に追加:
```
埋め込みAPIの月間リクエスト数を制限しますか？ (y/N)
→ Yes: 上限値を入力してください (デフォルト: 1000)
→ No: そのまま続行
```

TTY のみ。非TTY ではスキップ。

#### Config Set (`cli/src/commands/config.rs`)

`shiotsuchi config set <key> <value>`:
- `embedding_usage.enabled` → bool
- `embedding_usage.monthly_limit` → u64

TOML ファイルを読み込み → 該当フィールド変更 → 原子的に書き戻し。

#### Config Reset-Usage (`cli/src/commands/config.rs`)

`shiotsuchi config reset-usage`:
- `UsageTracker::reset()` を呼び出し
- "Usage counter reset for {month}" を表示

#### Tide Display (`cli/src/commands/tide.rs`)

既存表示の後に追加:
```
Embedding API: 42/1000 requests (2026-06)
```

`enabled=false` の場合は表示しない。

## Error Handling

| シナリオ | 処理 |
|---------|------|
| JSON ファイルが存在しない | 初回実行として作成 |
| JSON パースエラー | ログ警告 + ファイルをリセット |
| ファイル書き込み失敗 | ログ警告のみ（チェックは通過） |
| `enabled=false` | `check_and_increment()` をスキップ |
| ONNX バックエンド | `usage_tracker` は `None` なので影響なし |
| ディレクトリ作成失敗 | `Err(Load)` を返す |

## Testing Strategy

### Unit Tests

- `test_usage_tracker_creates_file`: 初回実行で JSON ファイルが作成される
- `test_usage_tracker_increments`: リクエストごとにカウンターが増加する
- `test_usage_tracker_monthly_rotation`: 月が変わるとカウンターがリセットされ、旧月が履歴に保存される
- `test_usage_tracker_limit_exceeded`: 上限到達時にエラーが返される
- `test_usage_tracker_disabled`: enabled=false ではチェックがスキップされる
- `test_usage_tracker_history_preserved`: 履歴が月の変わり目でも保持される
- `test_usage_tracker_corrupted_file`: 壊れた JSON でもリカバリできる
- `test_usage_tracker_reset`: リセット後にカウンターが 0 になる
- `test_embedder_error_display`: `UsageLimitExceeded` のエラーメッセージが正しい

### Integration Tests

- `test_embed_batch_respects_usage_limit`: API 埋め込みで上限エラーが伝播する
- `test_index_stops_on_usage_limit`: `shiotsuchi index` で上限到達時に処理が中断する

## Migration

新しい `UsageTracker` は独立したファイルを扱うため、DB マイグレーションは不要。初回実行時に `usage.json` が自動作成される。
