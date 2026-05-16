# 設計: CLI ビルド時情報表示

## 概要

`shiotsuchi` CLI にビルド時の機能フラグの可視性を追加し、ユーザーが現在のバイナリにどの機能がコンパイルされているかを確認できるようにします。これにより、`--no-default-features` やビルド時環境変数によってランタイムの挙動が変わっていても、ユーザーがそれを発見できないという問題を解消します。

## 目標

1. `shiotsuchi -h` / `--help` のフッターに、**拡張**ビルド情報（watcher, async-index, model-embedded）を表示する。
2. `shiotsuchi -V` / `--version` にも同じ拡張情報を含める。
3. `shiotsuchi support` で、すべてのコンパイル時条件（Cargo features、依存クレートの features、モデルハッシュ、ランタイムパス、ヘルスチェック）を**網羅的**に出力する。
4. `support` に `--json` 出力を提供し、プログラムから利用可能にする。

## 非目標

- 実行時の機能オンオフ切り替え。
- MCPサーバーの help/version 出力の変更（今回のスコープ外）。
- 完全なクレート依存ツリー（`cargo tree` 相当）の表示。

## アーキテクチャ

### コンパイル時のフィーチャー検出

Rust の `cfg!()` マクロはコンパイル時にゼロコストでフィーチャーを検出できます。`env!("CARGO_FEATURE_...")` は optional フィーチャーに対して信頼できないため、`cfg!()` による論理値チェック、または小さな `build.rs` で `$OUT_DIR` に定数を書き出す方法を使います。

### 情報の層分け

| 層 | トリガー | 内容 |
|---|---|---|
| 拡張 | `-h`, `-V` | `watcher`, `async-index`, `model-embedded` |
| 網羅的 | `support` サブコマンド | 拡張情報 + `ort-download-binaries`, `vaporetto` 機能、SQLite bundled、ランタイムパス、モデルハッシュ検証 |

### データソース

| データ | ソース |
|---|---|
| `watcher` | `cfg!(feature = "watcher")` |
| `async-index` | `cfg!(feature = "async-index")` |
| `model-embedded` | `shiotsuchi_core::EMBEDDED_PREDICTOR_HASH`（空でない = 埋め込み済み） |
| `ort-download-binaries` | `ort` クレートの `cfg!(feature = "download-binaries")`（`build.rs` で検出） |
| `vaporetto` 機能 | `vaporetto` クレートの `cfg!(feature = "charwise-pma")` 等（`build.rs` で検出） |
| SQLite bundled | `rusqlite/bundled` の有無（`build.rs` で検出） |
| ランタイムパス | `ShiotsuchiConfig::load()` |
| モデルハッシュ | `resolve_model_path()` + `verify_model_hash()` |
| Config 設定値 | `ShiotsuchiConfig` の各フィールド（`support` コマンドのみ） |

## コンポーネント

### 依存関係と検出方法

`cli` クレートは `core` クレートに対する path dependency のみを持つため、`core` の transitive dependency（`vaporetto` や `ort` など）の feature flags を直接的に `cfg!()` で見ることはできません。そこで `core` クレート自身がビルド情報を公開定数として提供し、`cli` はそれを参照します：

```rust
// core/src/build_info.rs (新規)
pub const HAS_MODEL_EMBEDDED: bool = cfg!(has_model_embedded);
pub const VAPORETTO_CHARWISE_PMA: bool = cfg!(feature = "charwise-pma"); // vaporetto経由
```

`core/build.rs` は `SHIOTSUCHI_EMBED_MODEL` 環境変数の有無に応じて、`cfg!(has_model_embedded)` に相当するフラグ（例：`--cfg has_model_embedded`）を `cargo:rustc-cfg` で出力します。あるいは、単に `EMBEDDED_PREDICTOR_HASH` が空文字列かどうかで判定してもよいです。

`cli/build.rs` は極力小さくします；`cli` 自身の Cargo features（`watcher`, `async-index`）は `cli` の `cfg!()` で検出可能です。

### `cli/src/commands/support.rs` (新規)

```rust
#[derive(Args, Debug)]
pub struct SupportArgs {
    #[arg(long)]
    json: bool,
}

pub fn run_support(args: &SupportArgs, cfg: &ShiotsuchiConfig) -> Result<(), Box<dyn std::error::Error>> {
    let info = BuildInfo::gather(cfg)?;
    if args.json {
        println!("{}", serde_json::to_string_pretty(&info)?);
    } else {
        info.print_table();
    }
    Ok(())
}
```

### `cli/src/main.rs` 修正

1. `Commands` enum に `Support(SupportArgs)` を追加。
2. `Cli` の `#[command(...)]` を更新：
   ```rust
   #[command(
       after_help = crate::build_info::help_footer(),
       long_version = crate::build_info::long_version(),
   )]
   ```

### `cli/src/build_info.rs` (新規)

`help_footer()` と `long_version()` 関数を提供し、`cfg!()` チェックと `build.rs` から生成された定数を使って整形済み文字列を返します。

## 出力形式

### `--help` フッター（拡張）

```text
Build features: watcher=enabled, async-index=enabled, model-embedded=yes
```

clap の `after_help` を使ってデフォルト help に追記します。

### `--version`（拡張）

```text
shiotsuchi 0.3.2
Guiding your path through the data tide.
Build features: watcher=enabled, async-index=enabled, model-embedded=yes
```

### `support` サブコマンド（網羅的）

デフォルトのテキスト出力：

```text
=== Build Features ===
watcher        : enabled (default)
async-index    : enabled
model-embedded : yes (hash: abc123...)

=== Dependency Features ===
ort download-binaries : enabled
vaporetto charwise-pma: enabled
vaporetto tag-prediction: enabled
vaporetto cache-type-score: enabled
vaporetto fix-weight-length: enabled
rusqlite bundled      : yes

=== Runtime Paths ===
notes_dir      : /home/user/Notes
db_path        : /home/user/.local/share/shiotsuchi/shiotsuchi.db
model_path     : /home/user/.local/share/shiotsuchi/model.onnx
model_hash     : abc123... (verified: ✓)

=== Config Settings ===
indexing.include_extensions : md, txt, org
indexing.auto_exclude_hidden: true
indexing.dynamic_threshold  : 0.80
indexing.exclude_dirs       : [node_modules, .git, target]
watcher.enabled             : true
watcher.debounce_ms         : 500
```

JSON 出力（`--json`）：

```json
{
  "build": {
    "watcher": true,
    "async_index": false,
    "model_embedded": true,
    "model_hash": "abc123..."
  },
  "dependencies": {
    "ort_download_binaries": true,
    "vaporetto_charwise_pma": true,
    "vaporetto_tag_prediction": true,
    "vaporetto_cache_type_score": true,
    "vaporetto_fix_weight_length": true,
    "rusqlite_bundled": true
  },
  "runtime": {
    "notes_dir": "/home/user/Notes",
    "db_path": "/home/user/.local/share/shiotsuchi/shiotsuchi.db",
    "model_path": "/home/user/.local/share/shiotsuchi/model.onnx",
    "model_hash": "abc123...",
    "model_hash_verified": true
  },
  "config": {
    "indexing": {
      "include_extensions": ["md", "txt", "org"],
      "auto_exclude_hidden": true,
      "dynamic_threshold": 0.80,
      "exclude_dirs": ["node_modules", ".git", "target"]
    },
    "watcher": {
      "enabled": true,
      "debounce_ms": 500
    }
  }
}
```

## エラーハンドリング

- `model_path` が解決できない場合は `"model_path: not found"` と表示し、 panic しない。
- ハッシュ検証で I/O エラーが発生した場合は `"model_hash: error reading file"` と表示。
- JSON シリアライズエラーは通常の `serde_json::Error` として伝播。

## テスト

- `BuildInfo::gather()` のユニットテスト（mock config 使用）。
- `help_footer()` と `long_version()` のフォーマットのユニットテスト。
- JSON 出力のラウンドトリップユニットテスト。
- 統合テスト：`shiotsuchi support --json` を実行し、期待されるキーを含む有効な JSON であることを検証。

## 作成・修正対象ファイル

| パス | 操作 |
|---|---|
| `cli/build.rs` | 新規作成 |
| `cli/src/build_info.rs` | 新規作成 |
| `cli/src/commands/support.rs` | 新規作成 |
| `cli/src/commands/mod.rs` | `pub mod support;` を追加 |
| `cli/src/main.rs` | `Support` バリアントを追加、`#[command(...)]` を更新 |
| `cli/Cargo.toml` | 必要に応じて `build = "build.rs"` を追加 |

## 未解決の問題

なし — 設計承認済み。
