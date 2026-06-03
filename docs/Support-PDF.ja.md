# PDF サポート

[English](Support-PDF.md)

shiotsuchi-search は PDF ファイルの内容をインデックスし、全文検索の対象にすることができます。2つの抽出方式を組み合わせて、テキスト埋め込み PDF とスキャン PDF の両方に対応します。

## 抽出方式の概要

| 方式 | 対象 | 技術 | Feature Flag | デフォルト |
|------|------|------|-------------|-----------|
| **Phase A: ネイティブ抽出** | テキスト埋め込み PDF | pdfium-render + XY-Cut | `pdf` | 有効 |
| **Phase B: VLM 抽出** | スキャン PDF（画像のみ） | edgequake-pdf2md + VLM API | `vlm` | 無効 |

### 処理フロー

```
PDF ファイル
    │
    ▼
Phase A: pdfium-render でテキスト抽出
    │
    ├── テキストあり → インデックスに登録
    │
    └── テキスト空（スキャン PDF）
            │
            ▼
        Phase B: VLM で画像→テキスト変換（有効な場合）
            │
            ├── 成功 → インデックスに登録
            └── 失敗/未設定 → メタデータのみで登録
```

## Phase A: ネイティブテキスト抽出

### 概要

[PDFium](https://pdfium.googlesource.com/pdfium/) エンジン（Chrome 内蔵）の Rust バインディングを使用して、PDF 内のテキストを直接抽出します。

### 技術的な詳細

1. **文字情報の取得**: pdfium-render が各ページから文字ごとの座標（x0, y0, x1, y1）とフォントサイズを返す
2. **行クラスタリング** (`cluster_to_lines`): y 座標の差がフォントサイズの 0.5 倍以内の文字を同じ行にまとめる
3. **XY-Cut レイアウト解析** (`xycut_to_text`):
   - 段組み検出: 水平方向の最大ギャップで左右のカラムを分離
   - 読書順復元: 上→下、左→右の順にテキストを並べ替え
   - タイトル検出: ページ幅の 80% 以上の行をタイトルとして特別処理
4. **Markdown 変換**: フォントサイズの比見込みで見出しレベルを決定（比 ≥ 1.5 → H1、≥ 1.2 → H2）

### 例: 段組 PDF の処理

```
┌─────────────┬─────────────┐
│  左カラム    │  右カラム    │
│  本文テキスト │  本文テキスト │
└─────────────┴─────────────┘
```

XY-Cut が水平ギャップを検出し、左カラムを先に処理してから右カラムを処理します。

### 設定

```toml
[indexing]
enable_pdf_extraction = true   # デフォルト: true
include_extensions = ["md", "markdown", "pdf"]  # pdf はデフォルトで含まれる
```

`enable_pdf_extraction = false` の場合、PDF ファイルは空のコンテンツでインデックスされます（ファイル自体は DB に登録されます）。

## Phase B: VLM ベース抽出

### 概要

Vision Language Model (VLM) を使用して、スキャン PDF（画像のみ）からテキストを抽出します。Phase A でテキストが空だった PDF に対してのみ実行されます。

### 対応プロバイダー

| プロバイダー | モデル例 | 特徴 |
|-------------|---------|------|
| OpenAI | gpt-4.1-nano | 高精度、コストあり |
| Anthropic | — | 高精度、コストあり |
| Google Gemini | — | 高精度、コストあり |
| Ollama | llava 等 | ローカル実行、コストゼロ |

### 設定

```toml
[vlm]
enabled = true
provider = "openai"           # openai / anthropic / gemini / ollama
model = "gpt-4.1-nano"
max_pages_per_doc = 50        # 省略時は全ページ処理
```

### API キーの設定

```bash
# 一般的な設定（推奨）
export SHIOTSUCHI_API_KEY="your-api-key"

# プロバイダー固有の設定
export OPENAI_API_KEY="your-openai-key"
export ANTHROPIC_API_KEY="your-anthropic-key"
```

### コスト目安

| プロバイダー | 50 ページあたりの概算 |
|-------------|---------------------|
| GPT-4.1 | 約 $0.40 |
| Amazon Nova Lite | 約 $0.01 |
| Ollama (ローカル) | ゼロ |

## インデックスの仕組み

### ハッシュ-based キャッシュ

PDF ファイルの内容は SHA-256 ハッシュで追跡されます。ファイルが変更されていない場合は再抽取がスキップされます。

```
PDF ファイル
    │
    ▼
テキスト抽出 → SHA-256 ハッシュ計算
    │
    ├── ハッシュ一致 → スキップ（既存のインデックスを使用）
    └── ハッシュ不一致 → チャンク分割 → インデックス更新
```

### チャンク分割

抽出されたテキストは既存の Markdown チャンカーで分割されます:
- 見出し（`#`/`##`/`###`）で分割
- 長いセクションは段落で分割
- 各チャンクに FTS5 エントリとオプションのベクトル埋め込みが作成される

## トラブルシューティング

### PDF がインデックスされない

1. `include_extensions` に `pdf` が含まれているか確認:
   ```toml
   [indexing]
   include_extensions = ["md", "markdown", "pdf"]
   ```

2. `enable_pdf_extraction` が `true` であるか確認:
   ```toml
   [indexing]
   enable_pdf_extraction = true
   ```

3. 再インデックスを実行:
   ```bash
   shiotsuchi index --notes-dir ~/Notes
   ```

### テキストが正しく抽出されない

- テキスト埋め込み PDF: Phase A が自動的に処理します
- スキャン PDF: Phase B (VLM) を有効にしてください
- 混合 PDF（一部テキスト、一部画像）: Phase A でテキスト部分を抽出し、残りは VLM で処理

### VLM 抽出が失敗する

1. API キーが設定されているか確認:
   ```bash
   echo $SHIOTSUCHI_API_KEY
   ```

2. 設定ファイルで VLM が有効か確認:
   ```toml
   [vlm]
   enabled = true
   ```

3. ログでエラーを確認:
   ```bash
   RUST_LOG=warn shiotsuchi index --notes-dir ~/Notes
   ```

## ビルドオプション

### PDF サポート付きビルド（デフォルト）

```bash
cargo build --features pdf
```

### VLM サポート付きビルド

```bash
cargo build --features vlm
```

### PDF サポートなしビルド

```bash
cargo build --no-default-features --features watcher,async-index,semantic
```

## 関連ドキュメント

- [ref/architecture.md](../ref/architecture.md) — アーキテクチャ概要
- [ref/cli.md](../ref/cli.md) — CLI コマンド一覧
- [CHANGELOG.md](../CHANGELOG.md) — リリース履歴
