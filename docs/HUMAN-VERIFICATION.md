# Human Verification Checklist

Manual end-to-end verification steps for shiotsuchi-search.
Run after `make build` succeeds and all automated tests pass.

---

## Setup

```bash
mkdir -p /tmp/shiotsuchi-test-vault

printf "# Meeting notes\n\nDiscussed the project plan.\n" \
  > /tmp/shiotsuchi-test-vault/meeting.md
printf "# Shopping list\n\nApples, bananas, milk.\n" \
  > /tmp/shiotsuchi-test-vault/shopping.md
printf "# プロジェクト計画\n\nRustとSQLiteを使った検索エンジンの設計。\n" \
  > /tmp/shiotsuchi-test-vault/plan.md
printf "# 会議メモ\n\n2026年4月の定例会議。アーキテクチャを議論した。\n" \
  > /tmp/shiotsuchi-test-vault/kaigi.md
printf "# アーキテクチャ\n\nVaporettoトークナイザとBM25検索の組み合わせ。\n" \
  > /tmp/shiotsuchi-test-vault/architecture.md
```

---

## 1. Build

```bash
make build
./target/release/shiotsuchi --version
```

- [ ] Build succeeds, 2 binaries in `target/release/` (`shiotsuchi`, `shiotsuchi-mcp`)
- [ ] `--version` output contains "Guiding your path through the data tide."

---

## 2. chart / dive / tide / log

```bash
./target/release/shiotsuchi chart \
  --notes-dir /tmp/shiotsuchi-test-vault \
  --db-path /tmp/shiotsuchi-test-vault/.db.sqlite3

./target/release/shiotsuchi dive "project" \
  --notes-dir /tmp/shiotsuchi-test-vault \
  --db-path /tmp/shiotsuchi-test-vault/.db.sqlite3

./target/release/shiotsuchi dive "project" --json \
  --notes-dir /tmp/shiotsuchi-test-vault \
  --db-path /tmp/shiotsuchi-test-vault/.db.sqlite3

./target/release/shiotsuchi dive "プロジェクト" \
  --notes-dir /tmp/shiotsuchi-test-vault \
  --db-path /tmp/shiotsuchi-test-vault/.db.sqlite3

./target/release/shiotsuchi dive "no-match-query" \
  --notes-dir /tmp/shiotsuchi-test-vault \
  --db-path /tmp/shiotsuchi-test-vault/.db.sqlite3

./target/release/shiotsuchi tide \
  --notes-dir /tmp/shiotsuchi-test-vault \
  --db-path /tmp/shiotsuchi-test-vault/.db.sqlite3

./target/release/shiotsuchi log \
  --notes-dir /tmp/shiotsuchi-test-vault \
  --db-path /tmp/shiotsuchi-test-vault/.db.sqlite3
```

- [ ] `chart` completes without error (`Indexed 5 files`)
- [ ] `dive "project"` returns `meeting.md`
- [ ] `--json` output is valid JSON
- [ ] `dive "プロジェクト"` returns `plan.md` (Japanese search works)
- [ ] `dive "no-match-query"` returns 0 results without error
- [ ] `tide` shows `total_notes: 5`
- [ ] `log` shows indexing history (5 notes, ISO8601 timestamps)

---

## 3. Error message

```bash
./target/release/shiotsuchi --db-path /tmp/nonexistent.db dive "test"
```

- [ ] stderr contains a message suggesting `shiotsuchi chart`

---

## 4. scan (file watcher)

```bash
./target/release/shiotsuchi scan \
  --notes-dir /tmp/shiotsuchi-test-vault \
  --db-path /tmp/shiotsuchi-test-vault/.db.sqlite3 &
sleep 1
printf "# New note\n\nauto-index test\n" > /tmp/shiotsuchi-test-vault/new.md
sleep 2
./target/release/shiotsuchi dive "auto-index" \
  --notes-dir /tmp/shiotsuchi-test-vault \
  --db-path /tmp/shiotsuchi-test-vault/.db.sqlite3
kill %1
```

- [ ] New file is picked up and indexed automatically
- [ ] `dive "auto-index"` returns `new.md`

---

## 5. XDG paths

```bash
./target/release/shiotsuchi chart --notes-dir /tmp/shiotsuchi-test-vault
ls ~/.cache/shiotsuchi/db.sqlite3

./target/release/shiotsuchi chart \
  --notes-dir /tmp/shiotsuchi-test-vault \
  --db-path /tmp/custom.db
ls /tmp/custom.db
```

- [ ] Default DB is created at `~/.cache/shiotsuchi/db.sqlite3`
- [ ] `--db-path` override creates DB at the specified path

---

## 6. Makefile

```bash
make help
make test
make integration-test
make install PREFIX=/tmp/shiotsuchi-install
ls /tmp/shiotsuchi-install/bin/
make uninstall PREFIX=/tmp/shiotsuchi-install
ls /tmp/shiotsuchi-install/bin/
make clean
```

- [ ] `make help` lists all targets
- [ ] `make test` — all Rust tests pass
- [ ] `make integration-test` — all Vitest MCP integration tests pass (15 tests)
- [ ] `make install` copies 2 binaries to `PREFIX/bin` (`shiotsuchi`, `shiotsuchi-mcp`)
- [ ] `make uninstall` removes the 2 binaries
- [ ] `make clean` removes `target/`

---

## 7. Claude Desktop MCP integration

Add to `~/Library/Application Support/Claude/claude_desktop_config.json`:

```json
{
  "mcpServers": {
    "shiotsuchi": {
      "command": "/path/to/target/release/shiotsuchi-mcp",
      "env": {
        "SHIOTSUCHI_NOTES_DIR": "/tmp/shiotsuchi-test-vault",
        "SHIOTSUCHI_DB_PATH": "/tmp/shiotsuchi-test-vault/.db.sqlite3"
      }
    }
  }
}
```

Restart Claude Desktop, then verify:

- [ ] "Search my notes for project" → `search_vault` is called, results returned
- [ ] "Search my notes for プロジェクト" → Japanese search returns `plan.md`
- [ ] "Read the content of meeting.md" → `read_full_note` is called, content returned
- [ ] "Show vault statistics" → `vault_status` returns note count and last indexed time
