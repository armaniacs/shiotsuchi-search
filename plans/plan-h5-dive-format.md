# Plan: Human-Readable `dive` Output Format

**Issue**: H5 — UI Expert (Checking Team)
**Severity**: High
**Status**: Plan only (not implemented)

## Problem

The `dive` command outputs search results only as raw JSON. Users get no immediate visual feedback — no file paths, titles, or snippets in a readable format.

### Current State

```bash
$ shiotsuchi dive "search term"
[{"path":"notes/project.md","title":"Project","snippet":"...","relevance":0.95}]
```

- Raw JSON output is machine-oriented
- No alignment, no color, no visual hierarchy
- Contrast with `tide` and `log` commands which use fixed-width table formatting

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

### Implementation

**`cli/src/commands/dive.rs` changes:**

1. **New enum**:
   ```rust
   #[derive(clap::ValueEnum, Clone, Debug)]
   enum OutputFormat {
       Table,
       Json,
       JsonPretty,
   }
   ```

2. **Add `--format` flag**:
   ```rust
   #[arg(long, value_enum, default_value_t = OutputFormat::Table)]
   format: OutputFormat,
   // Keep existing `--json` for backward compat (overrides format)
   #[arg(long)]
   json: bool,
   ```

3. **New function `print_table(results, query, elapsed)`**:
   - Calculate column widths based on terminal width (or use `term_size` crate)
   - Print header with query text
   - For each result: print title/socre header, indented path, indented snippet (truncated to 3 lines)
   - Print footer with count and timing

4. **Backward compatibility**:
   - `--json` flag overrides `--format` to `Json`
   - Default behavior (`dive "query"`) outputs table format

### Dependencies

- **`term_size`** or **`terminal_size`** crate for terminal width detection (optional, fallback to 80 chars)
- None required — simple format strings suffice for basic implementation

### Color Support (Optional Enhancement)

- Title: Bold + color (using `ansi_term` or manual ANSI codes)
- Path: Dimmed
- Score: Yellow highlight for high scores (>0.8)
- Snippet match keyword: Underline or highlight

### Testing

- Unit test `print_table` output format for known inputs
- Test `--format json` produces valid JSON
- Test backward compatibility: `--json` flag still works and produces JSON
- Test empty results table format
- Test results with very long paths/titles (truncation)

## Trade-offs

| Approach | Changes | Pros | Cons |
|----------|---------|------|------|
| **A: Only table (replace JSON)** | ~50 lines | Simplest, best UX | Breaking change for scripts using JSON |
| **B: --format flag** (recommended) | ~100 lines | Backward compatible, extensible | Slightly more code |
| **C: Use external pager/formatter** | ~20 lines | Minimal code change | User must install separate tool, inconsistent UX |

## File Changes

| File | Change |
|------|--------|
| `cli/src/commands/dive.rs` | Add `OutputFormat`, `--format` flag, `print_table()` function |
| `cli/src/main.rs` | Update `dive` match arm to pass format flag |
| `core/src/search.rs` | (Possibly) expose elapsed time for footer |
| `Cargo.toml` (cli) | No new deps needed for basic implementation |

## Not In Scope

- Interactive paging (pipe to `less` is sufficient)
- Rich terminal UI (tui-rs)
- Search result caching
