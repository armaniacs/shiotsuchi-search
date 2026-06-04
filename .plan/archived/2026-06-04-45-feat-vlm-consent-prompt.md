# PBI-45: VLM 有効化時の同意プロンプト

## ユーザーストーリー
shiotsuchi 利用者として、VLM 機能を有効化するときに PDF の内容が外部 API（OpenAI/Anthropic/Gemini 等）に送信されることを明示的に確認したい、なぜなら機密文書をうっかりサードパーティに送信するリスクを避けたいから

## ビジネス価値
- GDPR/個人情報保護法への適合
- ユーザーのインフォームドコンセント確保
- サポート負荷低減（「VLM って何に使われるの？」を事前回答）

## 発端
Checking Team レビュー（`plans/2026-06-03-1354-review-serve-pdf.md`）の Medium 指摘。VLM モジュールが PDF 内容を外部 API に送信するが、ユーザー同意・開示が不十分。

## 前提条件
- VLM feature が実装済み（PBI-28）
- `[vlm]` config セクションが存在する（`provider`, `model`, `endpoint` 等）
- `shiotsuchi index`（chart）コマンドが存在する

## 設計判断（Linear DEV-39 コメントより）

> **同意文には送信先のエンドポイント URL を含める。**
>
> @iron: 「同意文には、送信先のエンドポイントのURLを含ませる。なぜなら、利用者にとって安全性の確保されたエンドポイントである場合もあるから。例：Amazon Bedrockの契約がある、企業で確保したLLMエンドポイントである、ollamaやLM studioを自分のためだけに動作させている等。」

つまり:
- 同意プロンプトは `VLM provider: {provider} ({endpoint})` のように実際のエンドポイントを表示する
- OpenAI / Anthropic のクラウドエンドポイントなのか、ollama localhost なのかが利用者にわかる
- `endpoint` の値は `VlmConfig` から取得（各 provider のデフォルトエンドポイントが解決された状態）

## BDD 受け入れシナリオ

```gherkin
Scenario: VLM 初回インデックス時に同意を促す（エンドポイント表示付き）
  Given VLM が有効（vlm.enabled = true）で初回の index 実行
  And provider が openai、endpoint が https://api.openai.com/v1
  When `shiotsuchi index` を実行する
  Then 「PDF の内容が VLM プロバイダに送信されます」と表示される
  And エンドポイント URL「https://api.openai.com/v1」が表示される
  And [y/N] を求められる

Scenario: 同意しない場合は VLM 無効で動作
  Given 同意プロンプトで N を選択
  When `shiotsuchi index` が続行される
  Then VLM 抽出はスキップされる
  And ログに「VLM extraction disabled by user consent」が記録される
  And インデックスは正常に完了する

Scenario: 同意済みの場合はプロンプトをスキップ
  Given `vlm.consent_obtained = true` が設定されている
  When `shiotsuchi index` を実行する
  Then 同意プロンプトは表示されない

Scenario: 同意は config に保存される
  Given プロンプトで Y を選択
  When index が完了する
  Then config に `vlm.consent_obtained = true` が書き込まれる

Scenario: VLM 無効の場合はプロンプト不要
  Given VLM が無効（vlm.enabled = false）
  When `shiotsuchi index` を実行する
  Then 同意プロンプトは表示されない

Scenario: 非 TTY 環境ではプロンプトをスキップ
  Given 非 TTY 環境
  And VLM が有効
  When `shiotsuchi index` を実行する
  Then 同意プロンプトは表示されない
  And ログに警告が記録される
```

## 受け入れ基準
- [ ] `VlmConfig` に `consent_obtained: bool` フィールド追加（デフォルト: false、シリアライズは `#[serde(skip_serializing_if = "is_false")]`）
- [ ] VLM 有効かつ初回の index 時に確認プロンプトを表示（`dialoguer::Confirm`）
- [ ] 同意プロンプトは**実際のエンドポイント URL** を含む（例: `https://api.openai.com/v1`、`http://localhost:1234/v1`）
- [ ] 同意後は config ファイルに `consent_obtained = true` を書き込む
- [ ] 非 TTY ではプロンプトを表示せず、ログに警告を記録
- [ ] 同意しない場合、VLM 抽出はスキップされ通常の index が続行される

## テスト戦略（TDD）

### ユニットテスト
- `test_vlm_consent_prompt_shown_on_first_run`
- `test_vlm_consent_prompt_skipped_when_consented`
- `test_vlm_consent_not_shown_when_vlm_disabled`
- `test_vlm_consent_not_shown_on_non_tty`

### 統合テスト
- モックされた TTY での同意フロー確認

## 実装アプローチ

### Config 拡張
```rust
// core/src/config.rs
pub struct VlmConfig {
    // ... existing fields ...
    /// VLM 外部 API 送信にユーザーが同意したか。
    /// 初回 index 時に dialoguer で確認し、同意後に true になる。
    #[serde(default, skip_serializing_if = "is_false")]
    pub consent_obtained: bool,
}
```

### 同意チェック箇所

`cli/src/commands/chart.rs` の `run_chart()` 内、VLM 有効チェック直後:

```rust
fn build_consent_prompt(vlm_cfg: &VlmConfig) -> String {
    let endpoint = resolved_endpoint(vlm_cfg); // provider ごとにデフォルトエンドポイントを解決
    format!(
        "VLM ({}) を使用して PDF の内容を以下のエンドポイントに送信します:\n  {}\n同意しますか？",
        vlm_cfg.provider, endpoint
    )
}

if vlm_cfg.enabled && !vlm_cfg.consent_obtained {
    if atty::is(atty::Stream::Stdin) {
        let theme = dialoguer_theme();
        let prompt = build_consent_prompt(&vlm_cfg);
        let consent = dialoguer::Confirm::with_theme(&theme)
            .with_prompt(&prompt)
            .default(false)
            .interact()?;
        if consent {
            // config に consent_obtained = true を書き込む
            save_config_with_consent(...)?;
        } else {
            log::warn!("VLM extraction disabled by user consent");
            vlm_cfg.enabled = false;
        }
    } else {
        log::warn!("VLM enabled but consent not obtained in non-TTY mode");
    }
}
```

### 既存の仕組みの流用
- `dialoguer_theme()` は `cli/src/util.rs` に既存
- Config の永続化は `cli/src/config.rs` の `save_config` パターンを流用
- エンドポイント解決ロジックは `core/src/vlm.rs` または `core/src/config.rs` の既存の `VlmConfig` から取得

## 見積もり
1 ポイント（半日）

## 技術的考慮事項
- `consent_obtained` は config.toml に保存される。一度同意すれば次回以降はスキップ
- consent が false から true に変わったときにのみ config を書き込む（不要な書き込みを避ける）
- `skip_serializing_if` により、false の場合は config に出力しない（ユーザーの config が汚れない）
- `dialoguer::Confirm` のデフォルトは `false`（安全側に倒す）

## 実装者向け注記

### 現状コードの確認
```bash
# VlmConfig の現状
grep -n "struct VlmConfig" core/src/config.rs -A 15

# dialoguer_theme の実装
grep -n "fn dialoguer_theme" cli/src/util.rs -A 5

# run_chart の既存実装
grep -n "fn run_chart" cli/src/commands/chart.rs -A 5
```

### 実装手順
1. `core/src/config.rs` の `VlmConfig` に `consent_obtained` を追加
2. `cli/src/commands/chart.rs` で同意確認ロジックを追加
3. 同意した場合の config 保存パスを実装
4. 非 TTY フォールバックを実装
5. テストを追加
6. `make test` で全テストパス確認

### 落とし穴
- config の保存は `shiotsuchi index` が root 権限で実行されている場合がある。保存先のパーミッションに注意
- hot-reload は不要（`shiotsuchi index` は1回の実行で完結する）
- `atty` クレートは既に `dialoguer` の推移的依存として存在する
