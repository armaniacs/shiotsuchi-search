# PBI-63: OpenTelemetry との統合（tracing → OTLP）— 1.0 以降 (DEV-68)

🚫 **この PBI は v1.0 リリースまで着手しないこと。**

## ユーザーストーリー

SRE として、tracing の span を OpenTelemetry Protocol (OTLP) でエクスポートできるようにしてほしい、なぜなら現状の stderr ログ出力では本番環境の集中ログ基盤（Loki / Datadog / CloudWatch 等）との統合に不十分で、サービスメッシュ全体での分散トレーシングが実現できないから

## ビジネス価値

- tracing イベントを stderr のテキストログに加えて OTLP でエクスポート可能になる
- OpenTelemetry Collector 経由で Loki/Tempo/Datadog 等に span を送信できる
- MCP・HTTP・CLI の全コンポーネントで一貫したトレーシング戦略を提供

## なぜ 1.0 まで着手しないか

| 理由 | 説明 |
|------|------|
| コスト対効果 | 現在の stderr + RUST_LOG で十分な運用が可能。OTLP は過剰 |
| 依存追加 | `opentelemetry` + `opentelemetry-otlp` + `opentelemetry_sdk` + `tracing-opentelemetry` の4クレート追加が必要 |
| 設定複雑化 | OTLP エンドポイント、認証、サンプリングレート等の設定項目が増える |
| メンテナンス | OpenTelemetry Rust SDK はまだ成熟途上（v0.x）で定期的な破壊的変更がある |
| 優先度 | 検索エンジンとしてのコア機能の充実（PBI-59〜62）が先 |

## 受け入れ基準

- [ ] **v1.0 リリース以降**に再評価すること
- [ ] v1.0 での再評価基準: `opentelemetry-rust` SDK が v1.0 に到達していること

## 保留中の設計メモ（再評価時の参考用）

### 実装イメージ

```rust
// CLI の tracing-subscriber 初期化に追加
use tracing_opentelemetry::OpenTelemetryLayer;

let tracer = opentelemetry_otlp::new_pipeline()
    .tracing()
    .with_exporter(opentelemetry_otlp::Exporter::new_default())
    .install_simple()?;

tracing_subscriber::registry()
    .with(OpenTelemetryLayer::new(tracer))
    .with(fmt_layer)
    .init();
```

### 設定項目（案）

```rust
pub struct TelemetryConfig {
    pub otlp_enabled: bool,
    pub otlp_endpoint: String,
    pub sampling_ratio: f64,    // 0.0 ~ 1.0
}
```

### ディレクトリ

- `core/src/telemetry.rs` — OpenTelemetry の設定・初期化

## Definition of Done（再評価時）

- [ ] 全 BDD シナリオが実装されパスする
- [ ] stderr ログと OTLP エクスポートが共存できる
- [ ] OTLP Collector 経由で Tempo / Loki に span が届くことを確認
- [ ] 依存クレートの CVE スキャンが通過
