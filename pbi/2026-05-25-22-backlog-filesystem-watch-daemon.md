# PBI: ファイルシステム監視による自動リアルタイムインデックス（Watch モード）

## ユーザーストーリー
Obsidian でノートを書きながら即座に検索したいユーザーとして、ノート保存時に自動でインデックスを更新してほしい、なぜなら手動で `chart` を実行するのが手間だから

## ビジネス価値
- ノート保存後すぐに検索結果に反映される
- `chart` 手動実行が不要になり日常操作コストを削減

## BDD 受け入れシナリオ

```gherkin
Scenario: ノートを保存すると自動でインデックスが更新される
  Given `shiotsuchi scan` がバックグラウンドで起動している
  When ユーザーが Obsidian でノートを保存する
  Then 5 秒以内にそのノートが検索結果に反映される

Scenario: ノートを削除するとインデックスから除外される
  Given `shiotsuchi scan` が起動中
  When ユーザーがノートを削除する
  Then そのノートが検索結果から消える
```

## 受け入れ基準
- [ ] `shiotsuchi scan` コマンドがファイル変更を監視してインデックスを自動更新する
- [ ] 作成・更新・削除イベントに対応する
- [ ] デバウンス（短時間に連続保存しても1回だけ処理する）を実装する

## 見積もり
5 ポイント

## 技術的考慮事項
- `notify` クレートで OS のファイルシステムイベントを監視（既存の `scan` コマンドを拡張）
- 影響ファイル: `cli/src/main.rs`、`core/src/indexer.rs`
- `scan` コマンドは既存？ → 詳細確認が必要

---

## ⚠️ 実装者向け注記

### 着手前の調査

```bash
# scan コマンドの現状実装を確認する
cat cli/src/commands/scan.rs
grep -n "notify\|watcher\|watch\|Watcher" core/src/ cli/src/ -r | head -20
# watcher feature の実装状況を確認する
grep -n "watcher\|async-index" core/Cargo.toml core/src/lib.rs
```

**`notify` クレートと `watcher` feature は既に `core/Cargo.toml` に含まれています。**  
`scan` コマンドも既存（`cli/src/commands/scan.rs`）です。

### 実装の焦点

1. **`scan` コマンドの現状実装を読んで何がすでに動いているか確認する**
2. **デバウンス処理の実装状況を確認する**  
   Obsidian は保存時に複数イベントを発火させる。デバウンス（300ms 程度）がないと同じファイルを連続インデックスする：
   ```rust
   // notify イベントを受け取ってから 300ms 待ち、追加イベントが来なければ処理する
   ```
3. **削除イベントのハンドリングを確認・実装する**  
   `EventKind::Remove(_)` でファイルが削除された場合に `db.delete_chunks_for_file(path)` を呼ぶ。

### 落とし穴

- macOS は `kqueue`（FSEvents）、Linux は `inotify` を使う。`notify` クレートはこれを抽象化してくれるが、macOS での再帰監視は設定が必要。
- `notify` の `RecommendedWatcher` はスレッドを起動する。CLI プロセスの Ctrl-C シグナルを受け取った時にクリーンにシャットダウンすること。
- ファイルが保存途中（書き込み中）のイベントを受け取る場合がある。ファイルオープンが成功してから処理すること（`fs::read` が失敗したらリトライするか無視する）。

## Definition of Done
- [ ] ファイル変更→インデックス更新の E2E テストがパスする
- [ ] コードレビュー完了
