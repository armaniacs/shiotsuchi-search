import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    // サーバー起動 + インデックス読み込みを考慮して 30 秒
    testTimeout: 30_000,
    hookTimeout: 30_000,
    // 同一 DB を参照するため並列実行しない（Vitest 4.x: poolOptions は廃止 → トップレベル）
    pool: "forks",
    singleFork: true,
  },
});
