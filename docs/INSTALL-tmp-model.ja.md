# テスト用モデルのインストール (日本語)

> このドキュメントは、shiotsuchi-search の **テスト/開発用** にモデルをセットアップするためのクイックガイドです。
> 本番利用には [INSTALL.md](./INSTALL.md) を参照してください。

## 手順

### 1. Vaporetto トークナイザーモデルのダウンロード

```bash
make model
```

これにより `models/bccwj-suw+unidic_pos+kana.model.zst` がダウンロードされます。

### 2. ビルド

```bash
make build
```

トークナイザーモデルがバイナリに埋め込まれ、`target/release/shiotsuchi` と `target/release/shiotsuchi-mcp` が生成されます。

### 3. インストール（オプション）

```bash
make install
```

バイナリが `~/.local/bin/`（または `~/.cargo/bin/`）にインストールされます。

### 4. 動作確認

```bash
# ヘルプ表示
shiotsuchi --help

# テストボールトの作成
mkdir -p /tmp/shiotsuchi-test-vault
echo '# Test Note' > /tmp/shiotsuchi-test-vault/test.md
echo 'Hello world' >> /tmp/shiotsuchi-test-vault/test.md

# インデックス作成
shiotsuchi chart --notes-dir /tmp/shiotsuchi-test-vault

# 検索
shiotsuchi dive "test" --notes-dir /tmp/shiotsuchi-test-vault

# 後片付け
rm -rf /tmp/shiotsuchi-test-vault
```

## クリーンアップ

```bash
make clean
```

## トラブルシューティング

| 症状 | 対処法 |
|------|--------|
| `make model` でダウンロード失敗 | `scripts/download-model.sh` を直接実行してエラーメッセージを確認 |
| `cargo build` でコンパイルエラー | Rust 1.75+ がインストールされているか確認: `rustc --version` |
| モデルファイルが見つからない | `ls models/` でファイルが存在するか確認 |

## 関連ファイル

- [`Makefile`](../Makefile) — ビルド・ダウンロードターゲット定義
- [`models/`](../models/) — モデルファイル格納ディレクトリ（`.gitignore` 管理）