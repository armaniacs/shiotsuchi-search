import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { Client } from "@modelcontextprotocol/sdk/client/index.js";
import { StdioClientTransport } from "@modelcontextprotocol/sdk/client/stdio.js";
import type { CallToolResult } from "@modelcontextprotocol/sdk/types.js";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { existsSync } from "node:fs";
import { mkdir, writeFile } from "node:fs/promises";

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
  
  // テスト用のボルトとインデックスを準備
  if (!existsSync(NOTES_DIR)) {
    await mkdir(NOTES_DIR, { recursive: true });
  }
  
  // サンプルノートを作成
  await writeFile(
    join(NOTES_DIR, "plan.md"),
    "# プロジェクト計画\n\nRustとSQLiteを使用したアーキテクチャ設計。"
  );
  await writeFile(
    join(NOTES_DIR, "meeting.md"),
    "# Meeting notes\n\nDiscussed the project plan.\n"
  );
  await writeFile(
    join(NOTES_DIR, "shopping.md"),
    "# Shopping list\n\nApples, bananas, milk.\n"
  );

  // インデックスが存在しない場合は構築
  if (!existsSync(DB_PATH)) {
    const shiotsuchiBin = join(REPO_ROOT, "target/release/shiotsuchi");
    if (!existsSync(shiotsuchiBin)) {
      throw new Error(
        `shiotsuchi バイナリが見つかりません: ${shiotsuchiBin}\n` +
          "先に make build を実行してください。"
      );
    }
    
    const { exec } = await import("child_process");
    await new Promise((resolve, reject) => {
      exec(
        `"${shiotsuchiBin}" chart --quiet --notes-dir "${NOTES_DIR}" --db-path "${DB_PATH}"`,
        { env: { ...process.env, SHIOTSUCHI_MODEL_PATH: join(REPO_ROOT, "models/bccwj-suw+unidic_pos+kana.model.zst") } },
        (error, stdout, stderr) => {
          if (error) {
            reject(new Error(`インデックス構築に失敗: ${error.message}\nstderr: ${stderr}`));
            return;
          }
          resolve(true);
        }
      );
    });
  }

  transport = new StdioClientTransport({
    command: BIN,
    env: {
      ...process.env,
      SHIOTSUCHI_NOTES_DIR: NOTES_DIR,
      SHIOTSUCHI_DB_PATH: DB_PATH,
      RUST_LOG: "info",
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
  if (transport) {
    await transport.close().catch(() => {});
  }
});

// ─── テスト ───────────────────────────────────────────────────────────────────

describe("tools/list", () => {
  it("4 つのツールを返す", async () => {
    const { tools } = await client.listTools();
    const names = tools.map((t) => t.name);
    expect(names).toContain("search_local_notes");
    expect(names).toContain("get_surrounding_context");
    expect(names).toContain("index_status");
    expect(names).toContain("rebuild_index");
    expect(tools).toHaveLength(4);
  });

  it("search_local_notes は query を required として定義する", async () => {
    const { tools } = await client.listTools();
    const tool = tools.find((t) => t.name === "search_local_notes")!;
    const required = (tool.inputSchema as { required?: string[] }).required ?? [];
    expect(required).toContain("query");
  });

  it("get_surrounding_context は chunk_id を required として定義する", async () => {
    const { tools } = await client.listTools();
    const tool = tools.find((t) => t.name === "get_surrounding_context")!;
    const required = (tool.inputSchema as { required?: string[] }).required ?? [];
    expect(required).toContain("chunk_id");
  });
});

describe("search_local_notes", () => {
  it("日本語クエリでヒットするノートを返す", async () => {
    const result = await client.callTool({
      name: "search_local_notes",
      arguments: { query: "プロジェクト", mode: "fts" },
    });
    const text = firstText(result);
    expect(result.isError).not.toBe(true);
    expect(text).toContain("RETRIEVED CONTEXT");
    expect(text).toContain("plan");
  });

  it("英語クエリでもヒットする", async () => {
    const result = await client.callTool({
      name: "search_local_notes",
      arguments: { query: "Rust SQLite", mode: "fts" },
    });
    const text = firstText(result);
    expect(result.isError).not.toBe(true);
    expect(text).toContain("RETRIEVED CONTEXT");
  });

  it("存在しないキーワードで空結果を返す（クラッシュしない）", async () => {
    const result = await client.callTool({
      name: "search_local_notes",
      arguments: { query: "zzznomatchxxx99999", mode: "fts" },
    });
    const text = firstText(result);
    expect(text).toContain("No results found");
    expect(result.isError).not.toBe(true);
  });

  it("空クエリで空結果を返す（クラッシュしない）", async () => {
    const result = await client.callTool({
      name: "search_local_notes",
      arguments: { query: "", mode: "fts" },
    });
    expect(result.isError).not.toBe(true);
  });

  it("結果にファイルパスとスコアが含まれる", async () => {
    const result = await client.callTool({
      name: "search_local_notes",
      arguments: { query: "アーキテクチャ", mode: "fts" },
    });
    const text = firstText(result);
    if (!text.includes("No results found")) {
      expect(text).toMatch(/Source \d+:/);
      expect(text).toMatch(/Score:/);
    }
  });
});

describe("get_surrounding_context", () => {
  it("存在しない chunk_id でエラーを返す（クラッシュしない）", async () => {
    await expect(
      client.callTool({
        name: "get_surrounding_context",
        arguments: { chunk_id: 999999, window: 1 },
      })
    ).rejects.toThrow();
  });
});

describe("index_status", () => {
  it("チャンク数を含む統計を返す", async () => {
    const result = await client.callTool({
      name: "index_status",
      arguments: {},
    });
    const text = firstText(result);
    expect(result.isError).not.toBe(true);
    expect(text).toMatch(/Total chunks/i);
    expect(text).not.toMatch(/Total chunks:\s*0/i);
  });

  it("DB サイズと Indexed files が含まれる", async () => {
    const result = await client.callTool({
      name: "index_status",
      arguments: {},
    });
    const text = firstText(result);
    expect(text).toMatch(/DB size/i);
    expect(text).toMatch(/Indexed files/i);
  });
});

describe("不明なツール", () => {
  it("存在しないツール名で例外を投げる", async () => {
    await expect(
      client.callTool({ name: "nonexistent_tool_xyz", arguments: {} })
    ).rejects.toThrow();
  });
});
