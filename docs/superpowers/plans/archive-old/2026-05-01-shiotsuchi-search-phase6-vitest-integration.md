# Shiotsuchi-Search Phase 6: Vitest による MCP 統合テスト

**Goal:** `@modelcontextprotocol/sdk` の `StdioClientTransport` を使い、`shiotsuchi-mcp` バイナリを子プロセスとして起動して実際の JSON-RPC プロトコルレベルで動作を検証する Vitest 統合テストを実装する。ユニットテストでは確認できない「クライアントから見たサーバーの振る舞い」を CI/CD で自動検証する。

**前提条件:**
- Phase 4 完了済み — `shiotsuchi-mcp` のリリースビルドが存在すること（`make build`）
- Node.js 20.11 以上（`import.meta.dirname` サポートのため）
- テスト実行前に Vault をインデックス済みであること（Task 2 参照）

**依存バージョン（2026-05-01 時点）:**
- `@modelcontextprotocol/sdk`: `^1.29.0`
- `vitest`: `^4.0.0`
- `typescript`: `^5.0.0`

**アーキテクチャ:**
```
Vitest (TypeScript)
  └── StdioClientTransport
        └── [child process] shiotsuchi-mcp
              └── obsidian-shiotsuchi-vault-core (SQLite + Vaporetto)
```

---

## 実装状況サマリー（2026-05-01 時点）

### ✅ 実装済み（Tasks 1–3 + Makefile）

- `integration/package.json` — `@modelcontextprotocol/sdk ^1.29.0` / `vitest ^4.0.0`
- `integration/tsconfig.json` — ESNext / bundler moduleResolution / strict
- `integration/vitest.config.ts` — 30秒タイムアウト / singleFork
- `integration/tests/mcp.test.ts` — 15テストケース（tools/list・search_vault・read_full_note・vault_status・不明ツール）
- `integration/node_modules/` — `npm install` 済み（136 packages）
- `Makefile` — `integration-test` ターゲットを追加

### ❌ 未実施（手動作業）

- Task 2: テスト用 Vault 作成とインデックス構築（`/tmp/shiotsuchi-test-vault/`）
- Task 3: `npm test` の実行と PASS 確認（バイナリと DB が必要）

---

## ファイル構成

```
integration/
├── package.json
├── tsconfig.json
├── vitest.config.ts
└── tests/
    └── mcp.test.ts
```

---

## Task 1: Node.js プロジェクトのセットアップ

- [ ] **Step 1: `integration/` ディレクトリを作成する**

```bash
mkdir -p integration/tests
```

- [ ] **Step 2: `integration/package.json` を書く**

```json
{
  "name": "shiotsuchi-integration-tests",
  "version": "0.1.0",
  "private": true,
  "type": "module",
  "scripts": {
    "test": "vitest run",
    "test:watch": "vitest"
  },
  "devDependencies": {
    "@modelcontextprotocol/sdk": "^1.29.0",
    "typescript": "^5.0.0",
    "vitest": "^4.0.0"
  }
}
```

- [ ] **Step 3: `integration/tsconfig.json` を書く**

```json
{
  "compilerOptions": {
    "target": "ES2022",
    "module": "ESNext",
    "moduleResolution": "bundler",
    "strict": true,
    "esModuleInterop": true,
    "skipLibCheck": true
  },
  "include": ["tests/**/*.ts", "vitest.config.ts"]
}
```

- [ ] **Step 4: `integration/vitest.config.ts` を書く**

```typescript
import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    // サーバー起動 + インデックス読み込みを考慮して 30 秒
    testTimeout: 30_000,
    hookTimeout: 30_000,
    // 同一 DB を参照するため並列実行しない
    pool: "forks",
    poolOptions: {
      forks: { singleFork: true },
    },
  },
});
```

- [ ] **Step 5: 依存をインストールする**

```bash
cd integration && npm install
```

---

## Task 2: テスト用 Vault とインデックスを準備する

統合テストは実バイナリを使うため、インデックス済みの Vault が必要。
`make build` 後に以下を実行する。

```bash
# Vault 作成（3ノート）
mkdir -p /tmp/shiotsuchi-test-vault

printf "# プロジェクト計画\n\nRustとSQLiteを使った検索エンジンの設計。\n" \
  > /tmp/shiotsuchi-test-vault/plan.md

printf "# 会議メモ\n\n2026年4月の定例会議。アーキテクチャを議論した。\n" \
  > /tmp/shiotsuchi-test-vault/meeting.md

printf "# アーキテクチャ\n\nVaporettoトークナイザとBM25検索の組み合わせ。\n" \
  > /tmp/shiotsuchi-test-vault/architecture.md

# インデックス構築
SHIOTSUCHI_MODEL_PATH=$(pwd)/models/bccwj-suw+unidic_pos+kana.model.zst \
  ./target/release/shiotsuchi chart \
    --notes-dir /tmp/shiotsuchi-test-vault \
    --db-path /tmp/shiotsuchi-test-vault/.db.sqlite3

# 確認
./target/release/shiotsuchi log \
  --notes-dir /tmp/shiotsuchi-test-vault \
  --db-path /tmp/shiotsuchi-test-vault/.db.sqlite3
```

期待: `Total: 3 notes` が表示される

---

## Task 3: 統合テストコードの実装

- [ ] **`integration/tests/mcp.test.ts` を書く（下記コード参照）**

- [ ] **動作確認**

```bash
cd integration && npm test
```

期待: 全テストが PASS する

---

## テストコード

`integration/tests/mcp.test.ts`:

```typescript
import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { Client } from "@modelcontextprotocol/sdk/client/index.js";
import { StdioClientTransport } from "@modelcontextprotocol/sdk/client/stdio.js";
import type { CallToolResult } from "@modelcontextprotocol/sdk/types.js";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { existsSync } from "node:fs";

// ─── 設定 ─────────────────────────────────────────────────────────────────────

// Node 20.11+ では import.meta.dirname が使えるが、フォールバックを用意する
const __dirname =
  typeof import.meta.dirname === "string"
    ? import.meta.dirname
    : dirname(fileURLToPath(import.meta.url));

const REPO_ROOT = join(__dirname, "../..");
const BIN = join(REPO_ROOT, "target/release/shiotsuchi-mcp");
const NOTES_DIR = "/tmp/shiotsuchi-test-vault";
const DB_PATH = "/tmp/shiotsuchi-test-vault/.db.sqlite3";

// ─── ヘルパ ───────────────────────────────────────────────────────────────────

/**
 * SDK 1.x の CallToolResult は { content: ContentBlock[], isError?: boolean }
 * content[0].type === "text" のとき text フィールドが存在する
 */
function firstText(result: CallToolResult): string {
  const block = result.content[0];
  if (block?.type === "text") return block.text;
  return "";
}

// ─── セットアップ / ティアダウン ──────────────────────────────────────────────

let client: Client;
let transport: StdioClientTransport;

beforeAll(async () => {
  if (!existsSync(BIN)) {
    throw new Error(
      `shiotsuchi-mcp バイナリが見つかりません: ${BIN}\n` +
        "先に make build を実行してください。"
    );
  }
  if (!existsSync(DB_PATH)) {
    throw new Error(
      `インデックス DB が見つかりません: ${DB_PATH}\n` +
        "Task 2 の手順でインデックスを構築してください。"
    );
  }

  transport = new StdioClientTransport({
    command: BIN,
    env: {
      ...process.env,
      SHIOTSUCHI_NOTES_DIR: NOTES_DIR,
      SHIOTSUCHI_DB_PATH: DB_PATH,
      RUST_LOG: "info", // stderr に出力されるので Inspector と同様に確認できる
    },
  });

  client = new Client(
    { name: "vitest-integration", version: "0.0.1" },
    { capabilities: {} }
  );

  await client.connect(transport);
}, 30_000);

afterAll(async () => {
  // 必ず close する（ゾンビプロセス防止）
  await transport.close().catch(() => {});
});

// ─── テスト ───────────────────────────────────────────────────────────────────

describe("tools/list", () => {
  it("3 つのツールを返す", async () => {
    const { tools } = await client.listTools();
    const names = tools.map((t) => t.name);
    expect(names).toContain("search_vault");
    expect(names).toContain("read_full_note");
    expect(names).toContain("vault_status");
    expect(tools).toHaveLength(3);
  });

  it("search_vault は query を required として定義する", async () => {
    const { tools } = await client.listTools();
    const tool = tools.find((t) => t.name === "search_vault")!;
    // inputSchema は JSON Schema object
    const required = (tool.inputSchema as { required?: string[] }).required ?? [];
    expect(required).toContain("query");
  });

  it("read_full_note は path を required として定義する", async () => {
    const { tools } = await client.listTools();
    const tool = tools.find((t) => t.name === "read_full_note")!;
    const required = (tool.inputSchema as { required?: string[] }).required ?? [];
    expect(required).toContain("path");
  });
});

describe("search_vault", () => {
  it("日本語クエリでヒットするノートを返す", async () => {
    const result = await client.callTool({
      name: "search_vault",
      arguments: { query: "プロジェクト" },
    });
    const text = firstText(result);
    const hits = JSON.parse(text) as { path: string }[];
    expect(hits.length).toBeGreaterThan(0);
    expect(hits.some((h) => h.path.includes("plan"))).toBe(true);
  });

  it("英語クエリでもヒットする", async () => {
    const result = await client.callTool({
      name: "search_vault",
      arguments: { query: "Rust SQLite" },
    });
    const hits = JSON.parse(firstText(result)) as { path: string }[];
    expect(hits.length).toBeGreaterThan(0);
  });

  it("存在しないキーワードで空配列を返す（クラッシュしない）", async () => {
    const result = await client.callTool({
      name: "search_vault",
      arguments: { query: "zzznomatchxxx99999" },
    });
    const hits = JSON.parse(firstText(result));
    expect(Array.isArray(hits)).toBe(true);
    expect(hits).toHaveLength(0);
    expect(result.isError).not.toBe(true);
  });

  it("空クエリで空配列を返す（クラッシュしない）", async () => {
    const result = await client.callTool({
      name: "search_vault",
      arguments: { query: "" },
    });
    const hits = JSON.parse(firstText(result));
    expect(Array.isArray(hits)).toBe(true);
    expect(hits).toHaveLength(0);
    expect(result.isError).not.toBe(true);
  });

  it("結果オブジェクトに path / title / snippet / score が含まれる", async () => {
    const result = await client.callTool({
      name: "search_vault",
      arguments: { query: "アーキテクチャ" },
    });
    const hits = JSON.parse(firstText(result)) as Record<string, unknown>[];
    if (hits.length > 0) {
      expect(hits[0]).toHaveProperty("path");
      expect(hits[0]).toHaveProperty("title");
      expect(hits[0]).toHaveProperty("snippet");
      expect(hits[0]).toHaveProperty("score");
    }
  });
});

describe("read_full_note", () => {
  it("存在するノートの全文を返す", async () => {
    const result = await client.callTool({
      name: "read_full_note",
      arguments: { path: "plan.md" },
    });
    const text = firstText(result);
    expect(result.isError).not.toBe(true);
    expect(text).toContain("プロジェクト計画");
    expect(text).toContain("Rust");
  });

  it("パストラバーサル（../）を isError: true で拒否する", async () => {
    const result = await client.callTool({
      name: "read_full_note",
      arguments: { path: "../../../etc/passwd" },
    });
    // SDK 1.x: ツールがエラーを返すと isError: true になる
    expect(result.isError).toBe(true);
    expect(firstText(result).toLowerCase()).toMatch(/invalid|error/);
  });

  it("絶対パスを isError: true で拒否する", async () => {
    const result = await client.callTool({
      name: "read_full_note",
      arguments: { path: "/etc/passwd" },
    });
    expect(result.isError).toBe(true);
    expect(firstText(result).toLowerCase()).toMatch(/invalid|error/);
  });

  it("存在しないファイルで isError: true を返す（クラッシュしない）", async () => {
    const result = await client.callTool({
      name: "read_full_note",
      arguments: { path: "nonexistent_file_xyz.md" },
    });
    // ファイルが存在しない → エラーが返るが、サーバーはクラッシュしない
    expect(result.isError).toBe(true);
  });
});

describe("vault_status", () => {
  it("インデックス済みノート数を含む統計を返す", async () => {
    const result = await client.callTool({
      name: "vault_status",
      arguments: {},
    });
    const text = firstText(result);
    expect(result.isError).not.toBe(true);
    expect(text).toMatch(/total notes/i);
    // 少なくとも 1 件インデックス済み
    expect(text).not.toMatch(/total notes:\s*0/i);
  });

  it("DB サイズと最終インデックス日時が含まれる", async () => {
    const result = await client.callTool({
      name: "vault_status",
      arguments: {},
    });
    const text = firstText(result);
    expect(text).toMatch(/db size/i);
    expect(text).toMatch(/last indexed/i);
  });
});

describe("不明なツール", () => {
  it("存在しないツール名で例外を投げる", async () => {
    // SDK 1.x: unknown tool は MCP エラーレスポンスになり、SDK が例外として throw する
    await expect(
      client.callTool({ name: "nonexistent_tool_xyz", arguments: {} })
    ).rejects.toThrow();
  });
});
```

---

## Makefile への追加（任意）

`Makefile` に以下を追加することで `make integration-test` で実行できる:

```makefile
integration-test: build
	cd integration && npm install --silent && npm test
```

`.PHONY` にも追加:
```makefile
.PHONY: build build-dev test bench install uninstall clean help model integration-test
```

---

## CI/CD への組み込み方針

GitHub Actions 等に組み込む際のポイント:

1. モデルファイル（約 30MB）はキャッシュに乗せる
2. `make build` でバイナリをビルドする（`SHIOTSUCHI_EMBED_MODEL` 必須）
3. `shiotsuchi chart` でテスト用 Vault をインデックスする
4. `cd integration && npm ci && npm test` を実行する

```yaml
# .github/workflows/integration.yml の骨格
- name: Cache model
  uses: actions/cache@v4
  with:
    path: models/
    key: model-${{ hashFiles('scripts/download-model.sh') }}

- name: Download model
  run: make model

- name: Build
  run: make build

- name: Setup test vault
  run: |
    mkdir -p /tmp/shiotsuchi-test-vault
    printf "# プロジェクト計画\n\nRustとSQLiteを使った検索エンジンの設計。\n" \
      > /tmp/shiotsuchi-test-vault/plan.md
    printf "# 会議メモ\n\n2026年4月の定例会議。アーキテクチャを議論した。\n" \
      > /tmp/shiotsuchi-test-vault/meeting.md
    printf "# アーキテクチャ\n\nVaporettoトークナイザとBM25検索の組み合わせ。\n" \
      > /tmp/shiotsuchi-test-vault/architecture.md
    SHIOTSUCHI_MODEL_PATH=models/bccwj-suw+unidic_pos+kana.model.zst \
      ./target/release/shiotsuchi chart \
        --notes-dir /tmp/shiotsuchi-test-vault \
        --db-path /tmp/shiotsuchi-test-vault/.db.sqlite3

- name: Integration tests
  run: cd integration && npm ci && npm test
```

---

## 完了条件

| 確認項目 | チェック |
|---------|---------|
| `npm test` が全テスト PASS する | ☐ |
| `search_vault` で日本語・英語検索が動作する | ☐ |
| 検索結果に `path` / `title` / `snippet` / `score` が含まれる | ☐ |
| 空クエリ・存在しないキーワードでクラッシュしない | ☐ |
| `read_full_note` でノート全文が取得できる | ☐ |
| パストラバーサル・絶対パスが `isError: true` で拒否される | ☐ |
| `vault_status` でノート数・DBサイズ・最終インデックス日時が返る | ☐ |
| `afterAll` でサーバープロセスが確実に終了する | ☐ |

---

## Next Steps

Phase 7: MCP Inspector によるブラウザ GUI インタラクティブテスト
