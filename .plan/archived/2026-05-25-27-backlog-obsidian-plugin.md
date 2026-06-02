# PBI: Obsidian コミュニティプラグイン化

## ユーザーストーリー
Obsidian を日常的に使うユーザーとして、CLI を使わずに Obsidian 内から高速検索を使いたい、なぜなら CLI は非エンジニアユーザーにはハードルが高く、Obsidian プラグインとして提供することで一般ユーザーにリーチできるから

## ビジネス価値
- Obsidian コミュニティへのリーチ拡大（潜在ユーザー数百万人規模）
- GUI フロントエンドにより非エンジニアでも使えるようになる
- Obsidian の検索体験を大幅に改善するブランド認知

## BDD 受け入れシナリオ

```gherkin
Scenario: Obsidian のコマンドパレットから検索できる
  Given shiotsuchi プラグインが Obsidian にインストールされている
  When ユーザーがコマンドパレットで "Shiotsuchi: Search" を実行する
  Then 検索モーダルが表示され、入力した語でノートを検索できる

Scenario: 検索結果のノートを Obsidian で開く
  Given 検索結果が表示されている
  When ユーザーが結果をクリックする
  Then 対象ノートが Obsidian で開かれる
```

## 受け入れ基準
- [ ] Obsidian プラグイン（TypeScript）として動作する
- [ ] バックエンドの shiotsuchi CLI/MCP サーバーと通信する
- [ ] 検索モーダルで日本語入力に対応する
- [ ] Obsidian コミュニティプラグインの審査基準を満たす

## 見積もり
40 ポイント（Epic レベル、複数スプリントに分割必須）

## 技術的考慮事項
- Obsidian プラグイン API（TypeScript）での実装
- バックエンド通信: MCP または HTTP ローカルサーバー経由
- Obsidian プラグインは独立リポジトリとして管理推奨
- 依存: Fix-3（マルチ Vault）、MCP 安定化が前提

---

## ⚠️ 実装者向け注記

### このPBIを始める前に

**Epic レベルの大型 PBI です。必ずシニアエンジニアと設計を相談してから着手してください。**  
複数スプリントに分割して進めること。

### 技術選定の前提知識

1. **Obsidian プラグインは TypeScript/JavaScript で書く**  
   Rust のバックエンドとは別物。フロントエンドが TypeScript・Rust バックエンドが分離した設計になる。

2. **バックエンドとの通信方法を決める**（選択肢）：
   - **HTTP ローカルサーバー**: shiotsuchi に `shiotsuchi serve --port 7171` コマンドを追加し、REST API を提供する。プラグインは `fetch()` で通信する。
   - **Node.js ChildProcess**: Obsidian プラグインから shiotsuchi CLI を子プロセスとして起動する。設定が複雑になる。
   - **MCP経由**: Obsidian が MCP クライアントになれるかどうかは現時点で未確定。

   **推奨**: HTTP ローカルサーバーアプローチ。最もシンプルで確実。

### 実装フェーズ分割（推奨）

| フェーズ | 内容 | 見積もり |
|---------|------|---------|
| A | Rust 側に HTTP サーバーモード追加（`shiotsuchi serve`） | 8pt |
| B | Obsidian プラグインの雛形作成（テンプレートから）| 5pt |
| C | 検索モーダルの実装 | 8pt |
| D | 検索結果からノートを開く機能 | 5pt |
| E | コミュニティプラグイン審査・提出 | 13pt |

### 参考リソース

- Obsidian プラグイン開発: https://docs.obsidian.md/Plugins/Getting+started/Build+a+plugin
- サンプルプラグイン: https://github.com/obsidianmd/obsidian-sample-plugin
- プラグイン審査ガイドライン: https://docs.obsidian.md/Plugins/Releasing/Plugin+guidelines

### 落とし穴

- Obsidian プラグインはサンドボックス環境で動作するため、ファイルシステムへのアクセスは `app.vault` API 経由のみ許可される。直接 `fs` モジュールは使えない。
- HTTP サーバーを立てる場合、ポートが既に使用中の場合のフォールバックを実装すること。
- コミュニティプラグインの審査には数週間〜数ヶ月かかる場合がある。審査基準（コードの安全性、プライバシー）を事前に確認すること。

## Definition of Done
- [ ] Obsidian での基本検索が動作する
- [ ] コミュニティプラグイン審査を通過する
- [ ] ドキュメント（インストール手順）が整備される
