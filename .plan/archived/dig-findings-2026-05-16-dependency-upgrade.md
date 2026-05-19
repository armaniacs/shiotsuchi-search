# Deep Dig Findings — Dependency Upgrade Plan 評価

**日付:** 2026-05-16
**評価対象:** `docs/superpowers/plans/2026-05-16-dependency-upgrade.md`

---

## 挑戦した仮定

| # | 仮定 | リスク | 発見 | 決定 |
|---|------|--------|------|------|
| A | 全依存関係アップグレードによるパフォーマンス改善が作業コストに見合う | 高 | sha2/thiserror はパフォーマンス改善ほぼゼロだが、依存関係を最新に保つこと自体に価値がある | 「全部やるけど後で」— 低価値タスクはdeferredプランに分離 |
| B | rusqlite 0.31→0.39 の移行は計画に書かれた修正パターンで十分対応可能 | 高 | 8メジャーバージョンの差があり、非互換変更の範囲が未確認 | 事前にCHANGELOGを読んでから着手 |
| C | mainブランチに直接作業してよい | 高 | push前とはいえ切り戻しリスクがある | `chore/upgrade-rusqlite-and-deps` ブランチを作成 |
| D | 各タスクを独立したコミットにする価値がある | 中 | 1 session sequential実行ならむしろ速い | このセッションで順次実行（サブエージェント不要） |
| E | PRレビューが必要 | 中 | ユーザーが単独作者のため不要 | PR作成せず直接push→merge |
| F | Criterionベンチマークが有意な差を検出できる | 中 | rusqlite以外のアップグレードはパフォーマンス影響が小さい（またはゼロ） | baselineは最初に取得。deferredタスクを含む最終比較は後日 |
| G | デッド依存関係の削除はgrepで確認済みで安全 | 低 | 問題なし | 計画通り実施 |
| H | thiserror 2はソース変更不要 | 低 | フォーマット文字列のvalidationが厳格化されている可能性あり | コンパイルエラーが出たらその場で対応 |

## 新たに発見したリスク

1. **PR不要＋直接push** — コードレビューの安全網がない。ただしユーザーが単独作者であり、依存関係アップグレードはリバートが容易なので許容。
2. **ベンチマークの陳腐化** — deferredタスク（sha2, thiserror）を後日実行した時点でbaselineが古くなる。deferredタスクの実施時に改めてbaselineを取り直すか判断が必要。
3. **rusqlite CHANGELOG調査のタイミング** — CHANGELOGで判明した破壊的変更が計画の修正想定を超える場合、計画の再調整が必要になる。
4. **Cargo.lockのコンフリクト** — 前半のアップグレードでCargo.lockが更新され、後半のdeferredタスク実行時までに他の変更が入るとコンフリクトが発生しうる。

## 未解決の疑問

1. **deferredプランの正確なスコープ** — sha2 + thiserror + notify(optional) + Task 7(post-benchmark) を deferred プランに含めるか。Task 7のベンチマーク比較はdeferredタスク完了後に改めて行うべきか。
2. **deferredプランの格納場所** — `docs/superpowers/plans/deferred/` に格納するか、`docs/superpowers/plans/` 直下に別ファイルとして置くか。

## 決定事項

1. **featureブランチを使用する:** `chore/upgrade-rusqlite-and-deps`
2. **前半（今すぐ実行）のタスク順序:**
   0. ベンチマークbaseline取得 → commit
   1. featureブランチ作成
   2. Task 1: dead-dep削除（pulldown-cmark, ndarray）→ commit
   3. Task 2: cargo update（パッチ更新）→ commit
   4. 事前調査: rusqlite CHANGELOG確認
   5. Task 5: rusqlite 0.31→0.39 → commit
   6. push → mainにマージ（PR不要）
3. **後半（deferred）のスコープ:** sha2 + thiserror + notify(optional) + 事後ベンチマーク → 別planファイルに分割
4. **実行方式:** このセッションで順次実行（サブエージェント駆動は使わない）
5. **PRは作成しない** — ユーザーが単独作者のため直接マージ
