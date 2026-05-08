# Plan: Human-Readable `dive` Output Format

**Issue**: H5 — UI Expert (Checking Team)
**Severity**: High
**Status**: Implemented (2026-05-08)

> **Review (2026-05-08): Worth implementing — the only H-plan that addresses a real user-facing pain point.**
>
> **Why implemented now:**
> - **Consistency gap** — `tide` and `log` already use human-readable tables. `dive` is the primary search command and was the odd one out with raw JSON.
> - **Low cost, high impact** — ~100 lines of Rust, no new dependencies, no risk to core logic. The improvement is immediately visible every time someone runs a search.
> - **Backward compatible** — `--json` flag preserved, `--format json`/`--format json-pretty` also available. Existing scripts and pipes (`| jq`) continue working.
> - **No data risk** — Pure display change. DB, search engine, tokenizer untouched.

## Problem

The `dive` command outputs search results only as raw JSON. Users get no immediate visual feedback — no file paths, titles, or snippets in a readable format.

### Before

```bash
$ shiotsuchi dive "search term"
[{"path":"notes/project.md","title":"Project","snippet":"...","relevance":0.95}]
```

- Raw JSON output was machine-oriented
- No alignment, no color, no visual hierarchy
- Contrast with `tide` and `log` commands which use fixed-width table formatting

### After (current)

```bash
$ shiotsuchi dive "search term"
Results for "search term"
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
  1. Project Plan                                                     [0.95]
     notes/project.md
     This project is about building a search engine

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
1 results found (0.042s)
```

### User Experience Goals

A user running `dive` should immediately see:
- Which files were found (path)
- The note title
- A relevant snippet with context
- Match quality (relevance score)
- Clear visual separation between results

## Design

### Approach B: `--format` Flag (Recommended)

Add a `--format` / `-f` flag to `dive` with three modes:
- `table` (default): Formatted table output
- `json`: Current raw JSON (existing behavior with `--json`)
- `json-pretty`: Pretty-printed JSON (current default)

This preserves backward compatibility while providing a much better default experience.

### Table Format Specification

```
Results for "search term"                         ← Header
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
  1. Project Plan                     [0.95]      ← Title + score
     notes/project.md                             ← Path (dimmed/italic)
     This project is about building a search      ← Snippet (first 1-2 lines)
     engine that can handle complex queries…
                                                  
  2. Team Meeting                      [0.82]      ← Next result
     notes/meeting.md                             
     We discussed the search feature and…
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
2 results found (0.042s)                          ← Footer
```

### Implementation (actual)

**`cli/src/commands/dive.rs`:**

1. **`OutputFormat` enum** — `Table`, `Json`, `JsonPretty`. Public, derives `clap::ValueEnum`.

2. **`--format` flag** on `DiveArgs`:
   ```rust
   #[arg(long, value_enum, default_value_t = OutputFormat::Table)]
   pub format: OutputFormat,
   #[arg(long)]
   pub json: bool,
   ```
   Plus `effective_format()` method: if `json == true` returns `Json`, otherwise returns `format`. This handles the `--json` → `Json` override cleanly without clap conflicts.

3. **`print_results(results, query, format, elapsed)`** — dispatches to `print_table`, `serde_json::to_string` (Json), or `serde_json::to_string_pretty` (JsonPretty).

4. **`print_table(results, query, elapsed)`**:
   - Fixed-width formatting (78-char separator line, no terminal-size crate needed)
   - Header line: `Results for "query"`
   - Separator: `━` × 78
   - Per result: `N. title [score]` on line 1, `path` indented on line 2, up to 3 snippet lines indented on subsequent lines, `…` if snippet exceeds 3 lines
   - Footer: separator + `N results found (Ts)`
   - Timing measured via `std::time::Instant` in `main.rs` around the `run_dive` call (no core API changes)

5. **Dependencies** — Zero added. `serde_json` was already present in the CLI crate. No `term_size` / `terminal_size` / `ansi_term` crates.

6. **Color support** — Not implemented (deferred, as noted in the plan). The implementation keeps formatting clean and readable without ANSI codes.

**`cli/src/main.rs`:**

- Dive handler wraps `run_dive` call with `Instant::now()` / `.elapsed()`
- Passes `args.effective_format()` and `elapsed` to `print_results`

### Testing (actual)

All 9 new tests pass (added to `cli/src/commands/dive.rs`):

| Test | What it verifies |
|------|------------------|
| `test_effective_format_default_is_table` | No `--json`, no explicit `--format` → `Table` |
| `test_effective_format_json_flag_overrides` | `--json` alone → `Json` |
| `test_effective_format_json_pretty` | `--format json-pretty` → `JsonPretty` |
| `test_dive_effective_format_json_overrides_explicit_format` | `--json` wins over `--format json-pretty` |
| `test_print_results_json_produces_valid_json` | Serde round-trip with `Json` format |
| `test_print_results_json_pretty_produces_valid_json` | Serde round-trip with `JsonPretty` format |
| `test_print_table_empty_results` | No panic on empty slice |
| `test_print_table_with_results` | No panic with normal data |
| `test_print_table_long_content_truncation` | No panic with 200-char title, deep path, 5-line snippet |

Plus existing `test_dive_returns_results` and `test_dive_empty_query_returns_empty` updated to use `OutputFormat::Json` explicitly (matching prior JSON-only behavior).

**Full test suite**: 102 passed (45 CLI + 51 core + 6 workspace-integration), 0 failed. Clean build, zero warnings.

## Trade-offs

| Approach | Changes | Pros | Cons |
|----------|---------|------|------|
| **A: Only table (replace JSON)** | ~50 lines | Simplest, best UX | Breaking change for scripts using JSON |
| **B: --format flag** ✅ **implemented** | ~100 lines | Backward compatible, extensible | Slightly more code |
| **C: Use external pager/formatter** | ~20 lines | Minimal code change | User must install separate tool, inconsistent UX |

## File Changes (actual)

| File | Change |
|------|--------|
| `cli/src/commands/dive.rs` | Added `OutputFormat` enum, `--format` flag, `effective_format()`, `print_table()`, 9 new tests. Rewrote `print_results()`. |
| `cli/src/main.rs` | Wrap dive call with `Instant` timing; pass `effective_format()` and `elapsed` to `print_results` |
| `core/src/search.rs` | Unchanged — timing handled entirely in CLI layer |
| `Cargo.toml` (cli) | Unchanged — no new dependencies needed |

## Not In Scope

- Interactive paging (pipe to `less` is sufficient)
- Rich terminal UI (tui-rs)
- Search result caching
