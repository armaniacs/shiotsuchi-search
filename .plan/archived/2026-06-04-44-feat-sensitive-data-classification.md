# PBI-44: 機密データ分類・取扱機構

## ユーザーストーリー
shiotsuchi 利用者として、インデックス内の機密情報（クレジットカード、メールアドレス、API キー等）を検出・マスキングできることがほしい、なぜなら MCP サーバー経由で機密データが LLM に送信されるリスクを減らしたいから

## ビジネス価値
- 機密情報の意図しない流出防止（特に MCP/HTTP API 経由）
- エンタープライズ導入要件の充足
- ユーザーのプライバシー保護

## 発端
Checking Team レビュー（`plans/2026-05-27-0530-review-improve-branch.md`）の High 指摘。機密データの分類・取り扱い機構が存在しない。MCP 経由で個人データがフィルタリングされずに露出するリスクがある。

## 設計判断（Linear DEV-38 コメントより）

> **TruffleHog の検出器パターンを調査・流用する。API クレデンシャル・各種 SaaS キーの検出をスコープに含める。**
>
> @iron: 「各種APIサービスやSaaSを利用するときのキー、API鍵、各種クレデンシャル等も検出し、それらもマスクする機能が必要と考える。」
>
> @iron: 「TruffleHogが有力な候補。800種類以上の検出器や、外部APIを用いた動的なアクティブ検証（実証済み・未実証・不明の分類）…TruffleHogをそのまま使うのではなく、コードを部分利用することになる。やややり過ぎかもしれない。」

つまり:
- 初版は TruffleHog の detector 定義を調査し、そのパターンを部分利用する
- 「やややり過ぎ」の懸念があるため、**全800種ではなく、主要なものだけを取捨選択**する
- 動的検証（API に実際にアクセスしてキーの有効性を確認）は初版スコープ外
- メールアドレス・電話番号等の PII に加え、API キー・クレデンシャルを重点的にカバーする

## 前提条件
- チャンクデータは FTS5/vec に格納済み
- MCP ツール `search_local_notes` / `get_surrounding_context` / `read_full_note` が存在する
- HTTP API `/api/v1/search` / `/api/v1/read` が存在する

## BDD 受け入れシナリオ

```gherkin
Scenario: デフォルトで機密検出が無効
  Given 設定に `sensitive_data_detection` が未設定
  When MCP で検索を実行する
  Then 結果はマスキングなしで返却される

Scenario: メールアドレスがマスキングされる
  Given `sensitive_data_detection = true` が設定されている
  And ノートに "連絡先: user@example.com" が含まれている
  When MCP で search を実行する
  Then 結果のスニペットに "user@example.com" が含まれない
  And 代わりに "[EMAIL]" または同様のプレースホルダが表示される

Scenario: API キーがマスキングされる
  Given `sensitive_data_detection = true`
  And ノートに "API_KEY=sk-1234abcd" が含まれている
  When MCP で read_full_note を実行する
  Then "sk-1234abcd" がマスキングされている

Scenario: マスキングパターンをユーザーが追加できる
  Given `sensitive_patterns = ["CUST-\d{6}"]` が設定されている
  And ノートに "CUST-123456" が含まれている
  When 検索結果を表示する
  Then "CUST-123456" がマスキングされている

Scenario: マスキングは MCP と HTTP API の両方で有効
  Given `sensitive_data_detection = true`
  When MCP search_local_notes を実行する
  And HTTP GET /api/v1/search を実行する
  Then 両方の結果でマスキングが適用されている
```

## 受け入れ基準
- [ ] `sensitive_data_detection` 設定（デフォルト: `false`）
- [ ] `sensitive_patterns` カスタムパターン設定
- [ ] 組み込みパターン: メールアドレス、電話番号、API キー（sk-*）、URL 認証情報
- [ ] マスキング関数: `core/src/sensitive.rs`（新規モジュール）
- [ ] MCP 応答でマスキングを適用（search/read/get_surrounding_context）
- [ ] HTTP API 応答でマスキングを適用（search/read/list content）
- [ ] CLI の table 出力ではマスキングしない（ローカル端末は信頼済み）

## テスト戦略（TDD）

### ユニットテスト（core/src/sensitive.rs）
- `test_mask_email_addresses`
- `test_mask_api_keys`
- `test_mask_phone_numbers`
- `test_custom_pattern`
- `test_mask_disabled_does_nothing`
- `test_mask_multiple_patterns_in_one_text`

### 統合テスト
- `test_mcp_search_masks_sensitive_data`
- `test_http_api_search_masks_sensitive_data`
- `test_masking_not_applied_in_cli`

## 実装アプローチ

### フェーズ1: TruffleHog 調査（着手時の最初のタスク）
```bash
# TruffleHog の detector 定義を調査
# https://github.com/trufflesecurity/trufflehog の detectors ディレクトリを参照

# 例: detector 定義の抽出方針
# - pkg/detectors/ 以下に各 detector が配置されている
# - 各 detector はキーパターン (regex) と検証ロジックを持つ
# - 初版では regex パターンのみを部分利用し、動的検証は行わない
```

**取捨選択基準:**
- 包含: 主要クラウド (AWS, GCP, Azure, GitHub, GitLab, Slack, Discord, OpenAI, Anthropic 等)
- 包含: 汎用 Secret パターン (base64, hex, jwt, PEM 等)
- 包含: PII (Email, 電話番号, クレジットカード)
- 除外: 日本未普及サービス、動的検証が必要なもの、false positive 率が極端に高いもの
- 判断が難しいものは調査フェーズでリストアップし、@iron に確認

### フェーズ2: 実装

#### 新規モジュール: `core/src/sensitive.rs`
```rust
pub struct SensitiveDataConfig {
    pub enabled: bool,
    pub custom_patterns: Vec<String>,
}

/// 機密データを検出しプレースホルダに置換する。
/// CLI 出力には適用しない（信頼済み環境）。
pub fn mask_sensitive_data(text: &str, config: &SensitiveDataConfig) -> String { ... }
```

#### 組み込みパターン
TruffleHog 調査結果に基づき決定。最低限以下を含む:

- **PII**: Email, 電話番号,URL 認証情報
- **API Keys**: OpenAI (`sk-...`), Anthropic (`sk-ant-...`), AWS Key, GitHub PAT, GitLab Token, Slack Token, Discord Token, Google API Key, Azure Key
- **Generic Secrets**: base64 encoded credentials, PEM keys, JWT, AWS ARN
- パターンは `core/src/sensitive_patterns.rs` に分離して管理し、TruffleHog の upstream 更新に追従しやすくする

#### 適用箇所
- `mcp/src/handler.rs`: `search_local_notes`, `get_surrounding_context`, `read_full_note`
- `core/src/server/handlers.rs`: search 応答、read 応答
- CLI: 適用しない

#### 設定
```toml
[sensitive_data]
detection = false
# カスタムパターン（オプション）
patterns = ["CUST-\\d{6}"]
```

## 見積もり
13 ポイント（8-10日、調査含む）

## 技術的考慮事項
- マスキングは **検索結果のスニペット・全文読み取り時** にのみ適用する。インデックス時の原本は変更しない
- パフォーマンス: 大量のチャンクへのマスキング適用はレイテンシに影響するため、設定が有効な場合のみ適用
- regex エンジン: `regex` crate を使用（デフォルト依存。`regex` は既に推移的依存として存在）
- 日本語テキスト内のパターンも検出できるよう Unicode 対応が必要
- MCP と HTTP API でマスキングロジックを共有するため、コアモジュールに実装する
- **TruffleHog の抱える 800+ detector の一部だけを移植する判断は、最初の調査タスクで行う**。すべてを移植するのは維持コストが高すぎる

## 実装者向け注記

### 現状コードの確認
```bash
# MCP ハンドラの search 応答
grep -n "search_local_notes\|read_full_note\|get_surrounding_context" mcp/src/handler.rs

# HTTP API ハンドラの search 応答
grep -n "impl.*Handler\|Json.*search\|Json.*read" core/src/server/handlers.rs

# 依存関係に regex が既にあるか
grep "regex" core/Cargo.toml
```

### 実装手順
1. `core/src/sensitive.rs` を作成（マスキング関数 + テスト）
2. `core/src/lib.rs` に `pub mod sensitive;` を追加
3. `core/src/config.rs` に `SensitiveDataConfig` を追加
4. `core/Cargo.toml` に `regex` 依存を追加（なければ）
5. MCP handler でマスキングを適用
6. HTTP API handler でマスキングを適用
7. テストを追加
8. `make test` で全テストパス確認

### 落とし穴
- マスキング後の文字数が元のテキストと異なるため、スニペット位置がずれる可能性がある。マスキング後にスニペットを再計算するか、マスキングをスニペット抽出後に適用する
- `read_full_note` は全文を返すため、マスキングコストが高い。注意
- パターンマッチングは誤検知（false positive）の可能性がある。設定で無効化可能であること
- クレジットカード番号（Luhn algorithm）のような高度な検出は将来拡張とし、初版は regex ベースで十分
