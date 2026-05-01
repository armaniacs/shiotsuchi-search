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

  it("パストラバーサル（../）を MCP エラーで拒否する", async () => {
    // Rust が Err(...) を返すと SDK は McpError(-32000) として throw する
    await expect(
      client.callTool({
        name: "read_full_note",
        arguments: { path: "../../../etc/passwd" },
      })
    ).rejects.toThrow(/invalid|error/i);
  });

  it("絶対パスを MCP エラーで拒否する", async () => {
    await expect(
      client.callTool({
        name: "read_full_note",
        arguments: { path: "/etc/passwd" },
      })
    ).rejects.toThrow(/invalid|error/i);
  });

  it("存在しないファイルで MCP エラーを返す（クラッシュしない）", async () => {
    await expect(
      client.callTool({
        name: "read_full_note",
        arguments: { path: "nonexistent_file_xyz.md" },
      })
    ).rejects.toThrow();
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
    await expect(
      client.callTool({ name: "nonexistent_tool_xyz", arguments: {} })
    ).rejects.toThrow();
  });
});
