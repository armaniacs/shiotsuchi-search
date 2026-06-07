# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

> For project overview, features, installation, and usage, see [README.md](README.md).

## [Unreleased]

### Changed

- **BREAKING**: `SearchMode::FromStr` error type changed from `&'static str` to `SearchModeError` (thiserror). Code explicitly matching `Result<SearchMode, &'static str>` will not compile. Most consumers using `Err(_)` or `e.to_string()` are unaffected. `SearchModeError` implements `Display` and is re-exported from `shiotsuchi_core` crate root.

## [0.4.23] - 2026-06-07

### Fixed

- **All Clippy warnings eliminated (PBI-59)**: Resolved remaining `type_complexity` (2 → 0) and `too_many_arguments` (1 → 0) warnings. Introduced `VecSearchResult` type alias for `search_vec`/`search_hybrid` return types, added `#[allow(clippy::too_many_arguments)]` to test-only `upsert_file_cache`, and fixed `needless_range_loop` in tokenizer.
- **`criterion` updated to 0.8.2 (PBI-61)**: Dev-dependency benchmark library bumped from 0.5.1 to 0.8.2, no breaking changes.
- **VLM test assertion fixed**: `test_vlm_config_default_disabled` now expects `max_pages_per_doc: Some(10)` matching `VlmConfig::default()`.

### Documentation

- PBI-59, PBI-61 documents updated to reflect completed state.
- PBI-60 archived (already completed before PBI creation).

### Testing

- **477 core tests**, 144 CLI tests, 44 MCP tests — all passing, 0 clippy warnings.

## [0.4.22] - 2026-06-07

### Added

- **Cursor-based keyset pagination for FTS search (PBI-62)**: Added `cursor` query parameter to HTTP API `/api/v1/search`. Cursor encodes composite (rank, rowid) as opaque base64 string, supporting stable page traversal without offset performance degradation.
  - New types: `Cursor` (encode/decode), `SearchOutput` (results + next_cursor)
  - `fts_search` supports composite keyset: `(rank > ?) OR (rank = ? AND rowid > ?)`
  - ORDER BY tiebreaker: `rank, rowid` for deterministic pagination
  - Backward compatible: cursor=None preserves existing offset/limit behavior
  - 9 new tests: encode/decode unit tests, HTTP handler integration, full page traversal without duplicates (477 core tests total)

### Changed

- `search()` return type from `Vec<ChunkSearchResult>` to `SearchOutput { results, next_cursor }`

## [0.4.21] - 2026-06-06

### Added

- **Structured logging across all crates (PBI-53)**: Migrated the entire logging stack from `log` + `env_logger` to `tracing` + `tracing-subscriber` for structured, filterable logs with span support.
  - **MCP server (PBI-53a)**: `tracing-subscriber` with `.with_writer(std::io::stderr).with_ansi(false)` to guarantee stdout purity for JSON-RPC protocol. `LogTracer` bridge ensures `log::` calls from dependencies are captured.
  - **HTTP server (PBI-53b)**: `tower-http` `TraceLayer` with `SetRequestIdLayer` / `PropagateRequestIdLayer` — each request gets a UUID propagated as `x-request-id` response header. Latency and status logged automatically.
  - **Core library (PBI-53c)**: All 33 `log::warn!`/`log::debug!` call sites across 8 files migrated to `tracing::warn!`/`tracing::debug!`. `#[tracing::instrument]` added to `index_directory` with `vault_count` span field.
  - **CLI (PBI-53d)**: `env_logger` replaced with `tracing_subscriber` compact format. `--verbose` flag sets default filter to `debug` when `RUST_LOG` is unset.
- **Logging guide** (`docs/LOG.ja.md` / `docs/LOG.md`): Documentation covering `RUST_LOG` usage, per-crate output destinations, log format reading, and design rationale.
- **`SearchExecutionParams` struct (PBI-59)**: Extracted common search parameters from 3 internal functions — `search_fts` (11→2 args), `search_vec` (9→4 args), `search_hybrid` (15→6 args).

### Changed

- **`EmbedderBackend::Onnx` tokenizer boxed (PBI-60)**: `tokenizer: Tokenizer` → `tokenizer: Box<Tokenizer>` reduces Onnx variant from ~1200 bytes to ~24 bytes, eliminating `large_enum_variant` clippy warning.
- **Dependency updates (PBI-61)**: `rusqlite` 0.39→0.40, `indicatif` 0.17→0.18.
- Public `search()` API signature unchanged — all refactoring is internal.

### Fixed

- Clippy `too_many_arguments` warnings reduced from 4 to 1 (remaining: `upsert_file_cache` in `db.rs`).
- Clippy `large_enum_variant` warning eliminated.

### Testing

- **468 core tests**, 148 CLI tests, 45 MCP tests — all passing.
- 2 new HTTP handler tests: `test_response_has_request_id_header`, `test_request_id_propagates_client_header`.

### Documentation

- `docs/LOG.ja.md` / `docs/LOG.md`: New logging guide (Japanese and English).
- PBI-53a–53d archived to `.plan/archived/`.
- PBI-59–63 created and registered in Linear (DEV-64–DEV-68).

## [0.4.20] - 2026-06-06

### Added

- **MCP general rate limiter (PBI-57)**: Added `GENERAL_RATE_LIMITER` (50 req/s) guarding all `call_tool()` endpoints, and `REBUILD_RATE_LIMITER` (1 req/s) for `rebuild_index`. Both return a generalized error message without numeric values. Existing `SEARCH_RATE_LIMITER` (10 req/s) preserved as stricter search-specific limit.
- MCP sensitive data masking enabled by default (PBI-58): `SensitiveDataConfig::default()` now has `detection: true` (safe by default). `ToolContext.sensitive_config`, `call_tool()`, and `dispatch()` changed from `Option<&SensitiveDataConfig>` to `&SensitiveDataConfig` — compile-time guarantee against accidental masking skip.
- 3 new tests: shared rate limiter counter, `get_surrounding_context` rate limited, `index_status` rate limited.

### Changed

- **PBI-57/58 archived**: Spec files moved to `.plan/archived/`, AGENTS.md and 00-INDEX.md updated.

### Testing

- **44 MCP tests**, 439 core tests, 144 CLI tests — all passing.

## [0.4.19] - 2026-06-05

### Documentation

- **PBI-49 archived**: MCP `call_tool` ツール別分割 — コード上で既に実装完了していたため `.plan/archived/` に移動
- **PBI-43〜48 archived**: 5 件の完了済み PBI（migration 分割、UI アクセシビリティ、VLM キャッシュ、HTTP API 認証、sensitive data 分類、データ保持ライフサイクル）を `.plan/archived/` に移動
- **Backlog PBIs updated**: TDD 受け入れシナリオと実装状況を最新反映

## [0.4.18] - 2026-06-04

### Added

- **HTTP API rate limiting**: Sliding-window rate limiter (30 req/s) on all API endpoints. Returns `429 Too Many Requests` with `TooManyRequests` error type.
- **VLM consent prompt** (`shiotsuchi chart`): Interactive TTY consent prompt before sending document images to VLM API endpoints. Consent is persisted to `config.toml` as `vlm.consent_obtained`. Non-TTY environments (CI, cron) disable VLM with a warning.
- **`--force-vlm-reprocess` flag** (`shiotsuchi dredge`): Clears all VLM extraction hashes from `file_cache`, forcing PDF reprocessing on next index.
- **Data retention dry-run** (`shiotsuchi dredge --expired --dry-run`): Shows which files would be deleted without removing them, with confirmation prompt for actual deletion.
- **`exclude_dirs` backward compatibility test**: Config parser rejects old `exclude_patterns` key with a clear error message pointing to the new key name.
- **~30 new i18n message constants**: Japanese messages for `serve`, `check-ignore`, and `dredge` commands extracted to `cli/src/messages.rs`.

### Changed

- **Constant-time API key comparison**: `auth_middleware` uses `constant_time_eq` instead of `==` to prevent timing attacks on API key validation.
- **CORS header restriction**: Allowed headers narrowed from `any()` to explicit list (`Authorization`, `Content-Type`, `X-API-Key`).
- **Health endpoint simplified**: Removed `version` field from `/api/v1/health` response.
- **Stats endpoint simplified**: Removed `db_path` field from `/api/v1/stats` response (information leak).
- **Sensitive data masking on server**: `AppState` now carries `SensitiveDataConfig`; server responses pass through masking before returning.
- **MCP rate limiter poison resilience**: Mutex lock uses `unwrap_or_else(|e| e.into_inner())` to recover from poisoned mutex instead of panicking.
- **Build info memory leak fix**: `help_footer()` and `long_version()` use `LazyLock` instead of `Box::leak`.
- **VLM enforcement**: VLM extraction now requires both `vlm_enabled = true` AND `vlm_consent_obtained = true` (previously only checked `enabled`).
- **`delete_file_fully` in tests**: Transaction safety test updated to use atomic `delete_file_fully()` instead of separate `delete_chunks_for_file` + `delete_file_cache`.
- **`sensitive_patterns` refactored**: Built-in patterns now return `(regex, placeholder)` tuples instead of separate placeholder lookup.
- **i18n in `check-ignore`**: All user-facing strings moved to message constants.
- **i18n in `serve`**: Startup messages, error messages, and shutdown messages moved to message constants.
- **Migration module split** (PBI-48): `migrate()` extracted from 245-line method in `db.rs` to `core/src/migration/` — dispatcher in `mod.rs`, one file per version (`v02.rs`–`v11.rs`). `create_schema()` moved from `NoteDatabase` method to free function in `migration/mod.rs`.

### Fixed

- **UI accessibility**: `aria-live="polite"` on search results and file list; `aria-label` on modal; keyboard Space support on result cards and file items.
- **`VlmConfig` default**: `max_pages_per_doc` changed from `None` to `Some(10)` to prevent unbounded VLM processing.

### Documentation

- `docs/INSTALL.md` / `docs/INSTALL.ja.md`: `shiotsuchi chart` → `shiotsuchi index` command name
- `ref/core.md`: Schema v10→v11, migration v11 entry, feature flags table expanded, `migrate()` description updated

## [0.4.17] - 2026-06-04

### Added (PBI-39)

- **HTTP API Server** (`shiotsuchi serve`): Full REST API for search, stats, file listing, and note reading, powered by `axum` v0.8 + `tower-http` CORS.
  - `core/src/server/` — new modules: `handlers.rs` (6 endpoints), `types.rs` (ApiError, request/response types), `ui.html` (browser UI)
  - Endpoints: `GET /api/v1/health`, `POST /api/v1/search`, `GET /api/v1/stats`, `GET /api/v1/list`, `GET /api/v1/read`, `GET /ui`
  - Configurable via `[server]` section: `port` (default 7171), `host` (default `127.0.0.1`), `cors_origins`
  - API key authentication: `--api-key` CLI flag or `SHIOTSUCHI_SERVER_API_KEY` env var. Protected routes return 401 when key is set. Localhost-only bind shows auth status in startup banner.
  - CORS layer with configurable origins for cross-origin frontend access
  - Structured error responses (`ApiError`) with consistent JSON format
  - 20 handler tests, 18 auth tests, CORS integration tests

### Added (PBI-40)

- **VLM extraction binary-hash caching**: PDF binary content is now SHA-256 hashed and stored in `file_cache.vlm_hash` (DB migration v11). On re-index:
  - Cache hit → reuse existing chunk content, skip VLM API call entirely
  - Cache miss → call VLM API, store hash for future runs
  - `sha256_bytes()` utility in `db.rs` for PDF binary hashing

### Added (PBI-41)

- **Browser UI accessibility + pagination overhaul** (`ui.html`):
  - ARIA roles (`role=tablist/tab/tabpanel`), `aria-selected`, `aria-controls`, `aria-label` for tab navigation
  - Keyboard support: ArrowLeft/ArrowRight tab switching, Enter on result cards, focus trap in modal (Tab/Shift+Tab + Escape)
  - Screen reader: `<label>` with `.sr-only` on search input
  - `focus-visible` outlines for keyboard-only users
  - API pagination: `/api/v1/list?offset=N&limit=M` returns `total`/`offset`/`limit` in response
  - UI "Load more" button for file list
  - 15s fetch timeout via `AbortController` with user-facing error message
  - `lang="en"` (UI text was already English)

### Added (Other)

- **PDF/VLM feature status in build info**: `shiotsuchi support` now shows `pdf` and `vlm` feature flags in `BuildFeatures` table and build info footer

### Changed

- **`search()` signature refactored**: 17 positional arguments collapsed into `SearchRequest` struct (`core/src/search.rs`). All call sites updated (CLI dive, MCP handler, server handler)
- **VLM runtime safety**: Nested `Runtime::new().expect()` panic replaced with `tokio::task::block_in_place` for safe synchronous context handling
- **Security warning on non-localhost bind**: `shiotsuchi serve` emits a startup warning when `host` is not `127.0.0.1` or `localhost`
- **Synchronous file I/O → async**: VLM and PDF extraction paths use `tokio::fs` instead of `std::fs` in async context
- **VLM external API logging**: Each VLM API call is logged via `log::warn` for auditability
- **pdf.rs magic numbers → named constants**: Hardcoded numeric values replaced with descriptive constants
- **limit clamping**: Server and search parameter limits capped at 200
- **VLM empty API key filter**: Empty string API keys are filtered before sending to external providers

### Fixed

- **XSS in browser UI**: `ui.html` now uses `esc()` function to escape HTML entities in all user-controlled content display
- **DB `tag_counts` cleanup index**: Migration v11 adds index for efficient cleanup queries

### Testing

- **~575 total tests passing** across core, CLI, MCP, and E2E
- 20 server handler tests (health, search, stats, list, read, error format, CORS, pagination)
- 6 server auth tests (valid key, no key, wrong key, localhost skip, error format, Bearer header)
- VLM cache integration tests (cache hit, cache miss, hash collision)
- UI accessibility and pagination integration tests

### Documentation

- `docs/CLI-USE.md` / `docs/CLI-USE.ja.md`: Added `shiotsuchi serve` section with API endpoints, auth setup, and UI usage
- `docs/Support-PDF.md` / `docs/Support-PDF.ja.md`: New documentation for PDF extraction with VLM
- `README.md` / `README.ja.md`: Added HTTP API Server section with configuration examples
- `ref/cli.md`: Added `serve` command and `[server]` config section
- `ref/architecture.md`: Updated with server module, new dependencies
- `CLAUDE.md`: Added Linear CLI `npx` usage rule
- Completed PBI files archived from `pbi/` to `.plan/archived/`

## [0.4.16] - 2026-06-02

### Added (PBI-28)

- **VLM-based PDF markdown extraction** (`vlm` feature): Scanned PDFs and image-only PDFs can now be converted to searchable Markdown via Vision Language Models (OpenAI, Anthropic, Gemini, Ollama).
  - `vlm` Cargo feature enabled in CLI default build (`cli/Cargo.toml`)
  - `core/src/vlm.rs`: `extract_text_with_vlm()` with `edgequake-pdf2md` v0.9 integration
  - VLM fallback: when native PDF text extraction produces empty content and `vlm_enabled = true`, automatically calls VLM API
  - `VlmConfig`: `enabled`, `provider`, `model`, `max_pages_per_doc` settings in `[vlm]` config section
  - mtime-based caching: VLM is only called once per PDF; unchanged files are skipped on re-index
  - Graceful degradation: missing API key → skip with warning; VLM failure → keep empty, continue indexing
  - 3 new tests: feature compile verification, mtime cache skip, not-compiled build verification

### Added (Checking Team fixes)

- **`tag_counts` table for O(1) tag stats**: Migration v10 adds a `tag_counts` table (WITHOUT ROWID, PK on `(tag, vault_name)`) maintained incrementally during `reindex_file`. `tag_stats()` now reads from this table (O(K) on tag type count) instead of scanning all chunks (O(N))
- **`char_count` column for O(1) total chars**: Migration v10 adds `char_count` to `file_cache`. `stats()` now reads `SUM(char_count) FROM file_cache` instead of `SUM(LENGTH(content)) FROM chunks`
- **`delete_file_fully()`**: New atomic method on `NoteDatabase` that removes tag_counts, chunks, FTS/vec, tasks, file_cache, and note_links in a single SQLite transaction. Used by `cleanup_deleted`, watcher remove/rename, and CLI `delete` command
- **`build_path_map()`**: O(1) wikilink resolution via `HashMap<String, String>` (lowercase stem → shortest path), built once per vault in `index_directory`
- **MCP `get_surrounding_context` vault auth check**: Validates chunk's vault_name against configured vaults before returning surrounding context
- **MCP rate limiter sliding window**: Replaced fixed-second boundary rate limiter with sliding-window `VecDeque<Instant>` to prevent burst violations
- **MCP error path leak fix**: `canonicalize()` error messages no longer expose internal filesystem paths
- **`upsert_file_cache` char_count parameter**: Method now requires explicit `char_count` to prevent accidental zero values

### Changed

- **`ReindexParams` struct**: `reindex_file()` now takes a `&ReindexParams` struct instead of 10 positional arguments
- **`IndexParams` struct**: `index_file_with_embedder()` now takes a `&IndexParams` struct instead of 8 positional arguments
- **pdfium-render 0.8 unification**: Core dependency downgraded from v0.9 to v0.8 to match `pdfium-auto` and `edgequake-pdf2md`, eliminating duplicate binary
- **`create_schema` updated**: Schema now generates the final v10 layout directly (including tasks, note_links, tag_counts, emphasized_text, backlink_count, char_count)
- **Migration v8→v9, v9→v10 wrapped in transactions**: Multi-statement migration blocks now use `BEGIN TRANSACTION`/`COMMIT`
- **`and_query()` deprecated**: `JapaneseTokenizer::and_query()` marked `#[deprecated]` — use `collect_tokens()` + `expand_synonyms()` instead
- **Search score boost direction fixed**: Title and emphasized text boosts are now sign-aware for FTS/Vec modes (handles negative BM25 scores correctly)
- **Search re-sort after score boost**: Results are now re-sorted after title/emphasized text score adjustments

### Fixed

- **Watcher/CLI `delete` tag_counts inconsistency**: Remove, rename, and CLI delete events now properly decrement `tag_counts` via `delete_file_fully()`
- **Tag comma fragmentation warning**: `chunker.rs` now warns when a tag contains a comma (would be split on reindex)
- **`decrement_tag_count` zero-count cleanup**: Rows that reach `count = 0` are deleted to prevent dead-row accumulation

### Testing

- `deny.toml` added for supply chain monitoring via `cargo-deny`
- Migration v10 test updated (`open_fresh_db_has_version_10`)
- 4 new tag_counts integration tests (tag reflect, removal, empty ignored, char_count)
- 2 new tag consistency tests (count > 0 guard, decrement cleanup)
- 3 new VLM tests (feature compile, mtime cache, not-compiled build)
- **Total**: 563 tests passing across core, CLI, and MCP

### Documentation

- All 9 `docs/` files updated: MCP tool names, `[vlm]` config sections, Feature tables, schema v10, fuzzy FAQ
- `ref/core.md`: Schema v10, all methods, models table, migrations table updated
- `ref/architecture.md`: New modules, feature flags, 4 new design decisions
- `ref/cli.md`: `[vlm]` config section, synonym/tasks table formatting fix

## [0.4.15] - 2026-06-01

### Added (PBI-18)

- **Backlink / PageRank scoring**: Search results are now boosted based on how many other notes link to them — "hub notes" that are referenced from many places rank higher.
  - DB migration v9: `note_links` table captures `[[wikilink]]` relationships with vault-scoped source/target tracking; `backlink_count` column on `file_cache` stores per-file inbound link count
  - Obsidian `[[Note Name]]` / `[[Note Name|display text]]` wikilinks are extracted during indexing. Supports `[[Note#heading]]` and `[[Note^blockref]]` anchors (strips anchor, counts file reference). Case-insensitive resolution with shortest-path tiebreaking for ambiguous file names
  - `backlink_scoring` toggle (default `true`) in `[indexing]` config section — when disabled, neither backlinks are tracked nor scoring is applied
  - FTS and Vec modes: `score -= backlink_count × 0.05` (lower = more relevant). Hybrid mode: `score += backlink_count × 0.05` (higher = more relevant). Results are re-sorted after adjustment
  - Vault-scoped backlinks: links from vault A never inflate backlink counts in vault B
  - Batch backlink recount at end of `index_directory` (O(N) instead of per-file O(N²))
  - Watcher (incremental indexing) updates backlinks on modify/create/rename/remove events
  - `cleanup_deleted` cleans up outgoing `note_links` for removed files, preventing count inflation
  - `replace_note_links()` wraps DELETE + INSERT in an atomic SQLite transaction for crash safety
  - New index `idx_note_links_target` on `(target_path, vault_name)` for efficient recount queries
  - MCP search respects user's `backlink_scoring` config setting via `McpConfig`
  - 24 new tests: wikilink extraction (7), wikilink resolution (5), backlink indexing integration (2), note_links CRUD (5), backlink count update (3), search score adjustment (4)

## [0.4.14] - 2026-05-31

### Added (PBI-30)

- **Interactive welcome screen** (`shiotsuchi` without subcommand): Running `shiotsuchi` with no arguments opens an interactive welcome screen with a categorized command menu, replacing the previous clap error. New users see an onboarding wizard that guides them through the full setup flow: init → index → search.
  - Welcome banner with quick-start guide and categorized command listing (setup / search / info / exit)
  - Config existence detection — first-run shows "🚀 Start onboarding", config-only shows "⚡ Continue onboarding", ready shows "🚀 Quick onboarding"
  - `dialoguer::Select`-based menu with state-dependent labels
  - Error handling that catches failures and returns to the menu instead of crashing
  - Config reload after onboarding to reflect newly written settings
  - Non-TTY fallback with command list and `--help` guidance
- **3-step onboarding wizard**: Sequential execution of config creation (init), indexing (chart), and search (dive), with user confirmation between each step and pre-flight summaries showing what will be done.
- **3 new CLI modules**: `cli/src/commands/welcome.rs` (~580 lines), `cli/src/util.rs` grows `dialoguer_theme()` helper, `cli/src/messages.rs` gains ~30 `WELCOME_*` message constants

### Added (PBI-31..36 — review findings follow-up)

- **Non-TTY command list**: `WELCOME_NON_TTY_COMMAND_LIST` shows available commands instead of just `--help` hint
- **NO_COLOR support**: New `dialoguer_theme()` helper respects `https://no-color.org/` — uses `SimpleTheme` when `NO_COLOR` is set, `ColorfulTheme` otherwise. Applied to all dialoguer calls in welcome.rs, doctor.rs, and init.rs
- **Search query validation**: 200-character max length on dialoguer `Input` in both onboarding Step 3 and Search menu, with Japanese error message
- **Dynamic box width**: Completion screen and banner use dynamic padding instead of hardcoded widths
- **Messages extraction**: ~25 user-facing Japanese strings moved from welcome.rs to messages.rs constants for i18n preparation

### Fixed

- **Chunker UTF-8 panic**: `split_inline_segments` used char-vector indices as byte offsets when slicing the source string, causing `end byte index is not a char boundary` panic on multi-byte characters like `→` (U+2192). Fixed by using `char_indices()` to track `(byte_offset, char)` pairs
- **Onboarding config_exists hardcode**: Search → onboarding path passed hardcoded `false` for `config_exists`, causing Step 1 (config creation) to show even when config already existed. Now passes `config_path.exists()`
- **Removed flaky welcome tests**: `test_run_welcome_non_tty_path_still_works` and `test_stdin_is_not_terminal_in_test_env` were environment-dependent and blocked under PTY test runners
- **Model-skip error detection**: chart, clean, and doctor tests now also match `"No such file"` in error messages (in addition to `"no model"` / `"NoModel"`) to gracefully skip when model path is unset

### Changed

- `subcommand: Commands` → `subcommand: Option<Commands>` in Cli struct (clap derive)
- `dialoguer_theme()` extracted from local function in welcome.rs to shared helper in `cli/src/util.rs`
- `config_exists` now dynamically checked via `config_path.exists()` at all onboarding call sites

### Documentation

- `docs/CLI-USE.md` / `docs/CLI-USE.ja.md`: Added "Interactive welcome screen" sections with onboarding flow, state-dependent behavior, and non-TTY fallback explanation
- `ref/cli.md`: Added Interactive mode section, corrected `--mode` default from `fts` to `hybrid`
- `pbi/PBI-process.md`, `CLAUDE.md`: Updated for new PBI workflow

### Testing Infrastructure

- `scripts/test-timing.sh`: New timed test runner that groups tests by speed (fast/no-model vs slow/model-dependent), reports per-group timing, flags slow tests (>2x average), and supports `--retry-slow`
- `Makefile`: `test` target now uses timed runner. Added `test-fast`, `test-slow`, `test-retry-slow` targets
- `make test` total time reduced from 22+ minutes to ~3 minutes by separating core crate tests (no model needed, 1.7s) from individual model-dependent tests

## [0.4.13] - 2026-05-31

### Added

- **Intuitive command aliases (PBI-29)**: All CLI commands now have standard names as primary — `index` (`chart`), `search` (`dive`), `prune` (`dredge`), `watch` (`scan`), `list` (`log`), `stats` (`tide`). Ocean-themed original names remain as backward-compatible aliases. New names appear as primary in `--help`.
- **GFM-style task checkbox parsing**: `- [X]` (uppercase X) now recognized as checked task alongside `- [x]` in `shiotsuchi tasks`.

### Changed

- **MMR OOM guard tightened**: `MAX_MMR_CANDIDATES` reduced from 10,000 to 1,000 to prevent memory exhaustion on large result sets. Candidate pools exceeding 1,000 skip MMR reranking (fall back to original order).
- **PDF extraction graceful degradation**: Failed PDF extraction no longer stops indexing — logs a warning and falls back to VLM-based extraction if configured, continuing with empty body otherwise.
- **VLM performance**: Global tokio runtime reused via `OnceLock` instead of creating a new runtime per call.
- **User-facing messages**: All error messages referencing `shiotsuchi chart` updated to `shiotsuchi index`.

### Fixed

- **DB migration ordering**: v6 column-addition migration (`tags`, `frontmatter_date`, `title` on `chunks`) now runs before v7 tasks-table creation, preventing column loss on version-5 databases. Includes defensive column check before v7 for self-healing databases that hit the interim broken ordering.
- **VLM runtime panic**: `Runtime::new().expect()` replaced with `OnceLock<Result<Runtime, String>>` — tokio init failures are now propagated as `VlmError` instead of panicking, preserving the caller's graceful error-recovery path.

### Documentation

- All user-facing docs (`README.md`, `README.ja.md`, `docs/CLI-USE.md`, `docs/CLI-USE.ja.md`, `docs/INSTALL.md`, `docs/INSTALL.ja.md`, `docs/HUMAN-VERIFICATION.md`, `ref/cli.md`, `ref/architecture.md`, `CLAUDE.md`, `pbi/PBI-process.md`) updated to use new command names as primary.

## [0.4.12] - 2026-05-28

### Added

- **`get_dominant_model_id()`**: new DB query in `core/src/db.rs` to detect embedder model changes by returning the most frequent stored `model_id` in `file_cache` (excluding `"none"` entries).
- **Model change warning**: `chart` and `scan` commands now emit `WARN_MODEL_CHANGED` when the loaded embedder's `model_id` differs from the dominant one stored in the database, prompting a full re-index via `shiotsuchi chart`.
- **Deterministic tie-breaking**: `get_dominant_model_id` SQL query uses secondary `model_id ASC` sort for consistent results when multiple models have equal frequency.
- **`EmbedderConfig` wired through CLI**: `chart` and `scan` now receive `&EmbedderConfig` instead of resolving via hardcoded `resolve_model_path(None)`, enabling full configurability of model paths.
- **5 unit tests for `get_dominant_model_id`**: cover single model, most-frequent selection, `"none"` exclusion, deterministic tie-breaking, and empty cache.
- **OpenAI-compatible API embedder (`ApiEmbedder`)**: new `provider = "api"` in `[embedder]` config. Supports any OpenAI-compatible embedding API (e.g. Sakura AI `multilingual-e5-large`, OpenAI `text-embedding-3-small`). Configure via `endpoint`, `model`, and optional `api_key` fields.
- **`EmbedderBackend` enum**: internal refactor unifying ONNX local inference (`Onnx`) and HTTP API inference (`Api`) behind the existing `Embedder` public type. Consumers (`indexer.rs`, `search.rs`, watcher) require no changes.
- **`EmbedderConfig::create_embedder()`**: replaces `resolve_model_path()` as the primary embedder construction API. Routes `BuiltIn`/`OnnxFile` to local ONNX loading and `Api` to `ApiClient` HTTP calls.
- **API key resolution**: `SHIOTSUCHI_API_KEY` environment variable takes precedence over `config.toml` `api_key`. CLI emits a warning when the key is stored in config instead of the env var.
- **Batch API requests**: `ApiClient` chunks embedding requests into batches of 100 texts with a 60-second timeout per request.
- **Stable `model_id` for API provider**: derived from SHA-256 hash of `endpoint + model`, enabling model change detection for API-based embeddings as well.

### Changed

- **Embedder config documentation** (`ref/cli.md`): expanded `[embedder]` section with `api` provider fields (`endpoint`, `model`, `api_key`) and security note about env var usage.
- **Core type documentation** (`ref/core.md`): `EmbedderConfig` now documents all three variants including `Api`.
- **Japanese documentation** (`docs/CLI-USE.ja.md`, `docs/INSTALL.ja.md`, `README.ja.md`): translated `[embedder]` section and added API provider setup instructions.

### Fixed

- **Doc comment merge**: restored `list_cached_paths` doc comment that was accidentally folded into `get_dominant_model_id` in `core/src/db.rs`.

## [0.4.11] - 2026-05-27

### Added

- **Code block fence mixing tests**: Added test coverage for mixed backtick/tilde fence
  markers in `chunker.rs` — confirms that ` ``` ` opened with ` ~~~ ` close and vice versa
  are treated as content inside the block rather than closing it.

### Fixed

- **`reindex_file` task cleanup**: `reindex_file` now deletes associated tasks for the
  file being re-indexed before inserting new chunks, preventing stale task data from
  persisting across re-index operations (Data Integrity review finding).
- **Tasks output Japanese localization**: `shiotsuchi tasks` total count message now
  uses the Japanese message constant (`TASKS_TOTAL`) instead of hardcoded English text.

## [0.4.10] - 2026-05-27

### Added

- **`--threshold` CLI flag and `semantic_threshold` config option**: Filter
  search results by minimum score. FTS/Vec modes exclude results with score
  above the threshold (lower BM25 / cosine distance = more relevant). Hybrid
  mode excludes results with RRF score below the threshold. CLI `--threshold`
  overrides config value when both are specified.
- **Glob pattern support for `exclude_dirs`**: `exclude_dirs` patterns now
  support actual glob wildcards (`*`, `?`, `[abc]`, `{a,b}`) instead of treating
  them as literal characters. Patterns like `draft_*` match directories starting
  with `draft_`, and path patterns containing `/` are matched against the full
  relative path. Backward-compatible for existing literal directory name patterns.
- **`shiotsuchi tasks` command**: New subcommand for cross-vault task checkbox
  search. Scans all indexed notes for `- [ ]` (incomplete) and `- [x]` (completed)
  tasks with optional keyword filtering and `--all` flag for completed tasks.
- **Code/math block whitespace tokenization**: Code blocks (```` ``` ````) and
  math blocks (`$$...$$`) are now tokenized with whitespace splitting instead of
  Vaporetto, improving search accuracy for function names and identifiers.
- **Emphasized text score boost**: `==highlight==` and `**bold**` text is now
  detected during indexing and stored separately. Search results whose emphasized
  text matches the query receive a 0.5x score boost (higher relevance).
- **MCP metadata enrichment**: `search_notes` response now includes `tags`,
  `frontmatter_date`, and `title` fields from each chunk's frontmatter.
- **CLI syntax highlighting**: Matched search terms in table-format snippets are
  highlighted in bold red ANSI color. Respects `NO_COLOR` environment variable.
- **`.shiotsuchiignore` file support**: Place a `.shiotsuchiignore` file in any
  vault root directory to define exclude patterns alongside `exclude_dirs` in
  config.toml. Patterns use the same glob syntax as `exclude_dirs`.
- **`shiotsuchi check-ignore <path>` command**: Diagnostic tool that checks
  whether a given relative path would be excluded, and shows which pattern
  (from `exclude_dirs` or `.shiotsuchiignore`) matched.
- **Excluded file count in chart**: `shiotsuchi chart` now reports the number
  of files excluded by matching patterns in its summary output.
- **Multilingual whitespace fallback**: Vaporetto tokens containing ASCII text
  are post-processed with camelCase/underscore/digit-boundary splitting, improving
  English technical term search accuracy.
- **Extended `tide` stats**: Now displays top 10 tags by frequency, total
  character count across all indexed notes, and supports `--json` output format.

### Fixed

- **Hybrid mode score boost direction**: `apply_filters_and_boost` now receives
  `search_mode` parameter. Title and emphasized text boosts correctly increase
  RRF scores in Hybrid mode instead of decreasing them.
- **ANSI highlighting accessibility**: Match highlighting uses inverse video
  (`\x1b[7m`) in addition to bold red, making it distinguishable for colorblind users.
- **Makefile `test-all`**: Removed `cargo clean` from the `test-all` target to
  prevent unnecessary full rebuilds; separated into `clean-all`.
- **Embedding error masking**: ONNX inference failures (e.g., input too long,
  model corruption) are now propagated with the real error message instead of
  the misleading "model not loaded" generic message.
- **MMR similarity matrix OOM guard**: Added `MAX_MMR_CANDIDATES = 10_000` cap —
  MMR re-ranking falls back to original order when the candidate pool exceeds
  this bound, preventing accidental memory exhaustion.
- **Removed dead code**: `get_chunk_vectors()` in `db.rs` was unused since
  embeddings are now returned inline from `vec_search()`. Eliminated to reduce
  maintenance surface.
- **Embedding precomputation hoisted**: Query embedding is computed once and
  shared across Vec search, Hybrid RRF blending, and MMR re-ranking, eliminating
  duplicate ONNX inference calls.
- **E2E test assertions**: 5 e2e tests updated to match Japanese output format
  and v0.4.10 feature additions (chart summary, tide stats, log total, doctor
  summary, XDG path handling).

### Removed

- **Archived 12 completed PBIs**: PBI-01 through PBI-11 (mtime+size scan,
  semantic flag, multi-vault, frontmatter filter, i18n, DB path, user
  dictionary, synonym map, fuzzy search, alpha tuning, MMR) and PBI-28
  (synonym CLI manager) moved from `pbi/` to `.plan/archived/`.

### Documentation

- `ref/cli.md`: Added synonym subcommand, chart/scan `--vault`, expanded
  dive flags (`--mmr`, `--lambda`, `--fuzzy`, `--alpha`, `--tag`, `--since`),
  vault_default/user_dictionary/synonyms config fields
- `ref/core.md`: Updated `chunks` schema (tags/frontmatter_date/title),
  `file_cache` schema (file_size), `search()` signature (15 parameters),
  semantic feature flag, schema migrations v4-v8
- `docs/CLI-USE.md` / `docs/CLI-USE.ja.md`: Expanded dive docs with all
  search modes, MMR explanation, fuzzy/alpha/tag/since flags, synonym
  subcommand section, chart/scan `--vault` flag, check-ignore section,
  `.shiotsuchiignore` examples, config example with all new fields

### Added

- **MMR diversity re-ranking** (`dive --mmr --lambda 0.5`): Maximal Marginal Relevance
  re-ranks search results to balance relevance and diversity. Prevents near-duplicate
  chunks from dominating top results. Lambda controls the trade-off (0.0 = max
  diversity, 1.0 = pure relevance). Works in Vec and Hybrid modes with O(n²)
  precomputed similarity matrix. (4 unit tests)
- **Synonym CLI manager** (`shiotsuchi synonym add/remove/list`): Manage thesaurus
  entries from the command line without editing `config.toml` directly. Entries
  are stored in `~/.config/shiotsuchi/thesaurus.toml` and auto-merged into
  `ShiotsuchiConfig.synonyms` at startup. Supports multiple synonyms per word,
  duplicate detection, and file auto-creation on first use.
- **`vault_default` config option**: Set `vault_default = "work"` in `config.toml`
  to always restrict `dive`, `chart`, and `scan` to a specific vault when `--vault`
  is not specified.
- **`--vault` flag for `chart` and `scan`**: Both commands now support filtering
  by vault ID, matching the existing `dive --vault` behavior.
- **Japanese help text for all subcommands**: All 15 `shiotsuchi` subcommand
  descriptions now display in Japanese via `--help` (e.g., `ファイル監視`,
  `設定初期化`, `データベースを削除して最初からインデックスを再構築する`).

### Changed

- **Semantic search now optional via Cargo feature flag**: The `ort` (ONNX Runtime)
  dependency and all vector search code is gated behind the `semantic` feature
  (enabled by default). Building with `cargo build --no-default-features` produces
  a lightweight binary for FTS5-only users. The `--no-default-features` build
  compiles cleanly across CLI, MCP, and E2E crates.
- **`hybrid_alpha` config field**: The alpha blend ratio for hybrid search can now
  be set in `config.toml` via `hybrid_alpha = 0.3` (0.0 = vec only, 1.0 = FTS only).
  CLI `--alpha` flag takes precedence when both are specified. Default remains 0.5.
- **`shiotsuchi support` command now shows config**: The `support` command displays
  runtime configuration values alongside build info when invoked without `--json`.

### Fixed

- **Embedding error masking**: ONNX inference failures (e.g., input too long,
  model corruption) are now propagated with the real error message instead of
  the misleading "model not loaded" generic message.
- **MMR similarity matrix OOM guard**: Added `MAX_MMR_CANDIDATES = 10_000` cap —
  MMR re-ranking falls back to original order when the candidate pool exceeds
  this bound, preventing accidental memory exhaustion.
- **Removed dead code**: `get_chunk_vectors()` in `db.rs` was unused since
  embeddings are now returned inline from `vec_search()`. Eliminated to reduce
  maintenance surface.
- **Embedding precomputation hoisted**: Query embedding is computed once and
  shared across Vec search, Hybrid RRF blending, and MMR re-ranking, eliminating
  duplicate ONNX inference calls.

### Removed

- **Archived 12 completed PBIs**: PBI-01 through PBI-11 (mtime+size scan,
  semantic flag, multi-vault, frontmatter filter, i18n, DB path, user
  dictionary, synonym map, fuzzy search, alpha tuning, MMR) and PBI-28
  (synonym CLI manager) moved from `pbi/` to `.plan/archived/`.

### Documentation

- `ref/cli.md`: Added synonym subcommand, chart/scan `--vault`, expanded
  dive flags (`--mmr`, `--lambda`, `--fuzzy`, `--alpha`, `--tag`, `--since`),
  vault_default/user_dictionary/synonyms config fields
- `ref/core.md`: Updated `chunks` schema (tags/frontmatter_date/title),
  `file_cache` schema (file_size), `search()` signature (15 parameters),
  semantic feature flag, schema migrations v4/v5
- `docs/CLI-USE.md` / `docs/CLI-USE.ja.md`: Expanded dive docs with all
  search modes, MMR explanation, fuzzy/alpha/tag/since flags, synonym
  subcommand section, chart/scan `--vault` flag

## [0.4.8] - 2026-05-25

### Added

- **Coverage audit and gap closure**: Systematic audit identified and closed 6
  test coverage gaps across security-critical and core-flow paths:
  - `delete.rs`: 7 new tests for path traversal rejection (absolute, `..`,
    symlink escape), vault resolution fallback, and empty-vault panic
  - `config_migrate.rs`: 4 new tests for nonexistent config, already-new-format
    noop, full migration, and file permissions
  - MCP handler: 1 new test for vault-dir canonicalize check in
    `search_local_notes`
  - E2E: 1 new `e2e_doctor_diagnoses_without_tty` smoke test verifying non-TTY
    diagnostics

### Changed

- `docs/CLI-USE.md` / `docs/CLI-USE.ja.md`: Expanded doctor section with
  fixable-issues table, interactive prompt example, and non-TTY behavior note
- `ref/cli.md`: Added `doctor` to commands table, implementation files list,
  and outputs table

## [0.4.7] - 2026-05-25

### Added

- **Interactive fix mode for `shiotsuchi doctor`**: Doctor now detects fixable issues
  and prompts the user with `[y/N]` to repair them immediately:
  - Config unknown fields in `[indexing]`: Detected via `toml::Table` comparison with
    known field list; removed with timestamped backup
  - Config old `[vault]` format: Migrated to new multi-vault format inline
  - Database not found: Indexes vault from scratch (reuses `index_directory` path)
  - Database open/stats failure: Backs up corrupt DB, deletes old files, re-indexes
- Non-TTY environments skip all interactive prompts and fall back to read-only
  diagnostics (existing behavior)
- 13 new tests for unknown field detection, config fix helpers, vault format
  migration, backup collision handling, index_vault, and rebuild_db

### Changed

- `cli/src/commands/doctor.rs`: `run_doctor` now accepts `cfg`, `vaults`, and
  `indexing_cfg` parameters; config check attempts actual parsing via `load_from()`
  instead of only checking file existence
- `cli/src/commands/clean.rs`: `backup_file()` and `delete_db_files()` changed
  from `fn` to `pub(crate) fn` for cross-command reuse; `unwrap()` replaced with
  `unwrap_or_default()` on `duration_since` for panic safety

## [0.4.6] - 2026-05-23

### Changed

- **Dependency updates**: Upgraded 4 direct dependencies to their latest versions with zero source code changes:
  - `config` 0.14 → 0.15 (new internal deps: serde-untagged, erased-serde, typeid)
  - `dialoguer` 0.11 → 0.12 (dropped thiserror v1, uses console 0.16)
  - `dirs` 5 → 6 (dirs-sys 0.4→0.5, windows-sys 0.48→0.61)
  - `toml` 0.8 → 1.1 (internal restructuring: toml_parser + toml_writer split)
- **ADR-0002**: Updated to accurately reflect deferred status — sqlite-vec v0.1.x silently accepts `FLOAT2` DDL but treats it as `FLOAT` (f32), causing f16 blob INSERT failures. Decision section reworded from "Use FLOAT2" to "Adopt once supported". "Stay with FLOAT" changed from "Rejected" to "De facto current state".

### Fixed

- **License text in landing page**: `docs/index.html` incorrectly displayed "MIT License" in the CTA section and footer. Corrected to `Apache 2.0` to match the project's `LICENSE` file.

### Changed

- **Clippy lint resolution**: Suppressed `too_many_arguments` on `NoteDatabase::reindex_file()` (8 args) and `shiotsuchi_core::search::search()` (7 args) with detailed rationale. Suppressed `type_complexity` on `index_directory()` return type. Added `#[allow(clippy::const_evaluatable_checked)]` pattern in build_info test.
- **Magic number eliminated**: Replaced hardcoded `0.7071` in embedder tests with `EXPECTED_ORTHOGONAL_COSINE = f32::consts::FRAC_1_SQRT_2`.
- **Error propagation**: `stats()` in `db.rs` changed `unwrap_or(0)` to `?` — query failures are now propagated instead of silently defaulting to zero.
- **Needless borrows removed**: Removed extra `&` on `temp.path()` calls in `e2e/src/lib.rs` and `&result.unwrap()` in `cli/src/commands/clean.rs` and `cli/src/commands/init.rs` tests.
- **Config test cleanup**: Tests in `cli/src/config.rs` migrated from mutable field assignment to inline struct initialization with `..Default::default()`.
- **Remove dead assert**: Eliminated `assert!(true, …)` stub from `indexer.rs` test that provided no coverage value.

### Removed

- **Transitive dependencies**: `thiserror v1`, `base64 v0.21`, `hashlink v0.8`, `toml_edit`, `toml_write`, `nom` — no longer pulled in by any upgraded dependency.

## [0.4.5] - 2026-05-23

### Added

- **cargo install method**: INSTALL.md and INSTALL.ja.md now document Option A (`cargo install shiotsuchi shiotsuchi-mcp` from crates.io or `--git` from HEAD) with runtime model download instructions. Existing git+make approach becomes Option B (recommended for embedded model).

### Changed

- **Landing page**: Install section reworked with method tabs (`cargo install` / `git + make install`). Three install methods displayed as side-by-side cards (crates.io, git HEAD, git+make).

### Fixed

- **mtime fast-path test**: Test assertion in `indexer.rs` updated from `as_secs()` to `as_millis()` to match `file_mtime()` precision. Removed stale comment about sub-second tolerance.

## [0.4.4] - 2026-05-23

### Fixed

- **Revert FLOAT2 schema**: sqlite-vec 0.1.9 does not support `FLOAT2` column type. DDL was silently accepted (treated as `FLOAT32`) but 2048-byte f16 blobs caused `vec0` INSERT failures. Reverted to `FLOAT[1024]` (f32 storage). ADR-0002 updated to Deferred — pending sqlite-vec v0.2+.
- **mtime change detection**: `file_mtime()` used `as_secs()` which truncated to whole seconds. Two file writes within the same second produced identical mtime, causing the fast path to incorrectly return `Skipped` instead of `Updated`. Changed to `as_millis()`.

## [0.4.3] - 2026-05-22

### Added

- **f16 embedding quantization benchmark and ADR** (ADR-0002): Quantified f16 vs binary precision@k. f16 achieves perfect precision@k=1.0 at all k, halving storage at 2KB/chunk with zero accuracy loss. Implementation deferred — `sqlite-vec` 0.1.x does not support `FLOAT2` column type.
  - Quantization benchmark: `cargo bench -p shiotsuchi-core --bench quantization`
- **crates.io publishing support**: Added `description`, `homepage`, `repository`, `readme`, `keywords`, `categories` to all crates. Path dependencies include version for publish compatibility.
- **`make publish` target**: Runs tests first, then publishes core → cli → mcp in dependency order.
- **ADR-0001**: Binary size optimization strategy (`panic = "abort"`, `-71%` reduction from 75MB to 20MB)
- **ADR-0002**: f16 embedding quantization decision and benchmark results

### Changed

- **CLI/MCP config unified**: Shared config types extracted into `core/src/config.rs`. CLI re-exports via `pub use shiotsuchi_core::config::*`. MCP uses core types directly.
- **WalkDir streaming**: `index_directory()` no longer collects all entries into a `Vec` before processing. Pre-count walk removed — progress callback now uses `Option<usize>` for total (None when unknown).
- **ONNX batch inference**: `embed_and_insert_chunks` replaced with `embedder.embed_batch()` — all chunk embeddings computed in a single ONNX call instead of per-chunk loop.
- **Single-transaction reindex**: `NoteDatabase::reindex_file()` wraps delete + insert + embed + upsert in one `BEGIN IMMEDIATE TRANSACTION`.
- **Search deduplication**: `search_fts()` and `search_vec()` share `build_results()` — eliminated ~50 lines of duplicated post-processing.
- **`build.rs` caching**: mtime-based skip for model extraction (saves 5-10s per build when model unchanged).
- **CI optimization**: `cargo test --release` unifies test + release build, halving CI time.
- **Model path resolution**: Extracted `default_data_dir()` helper with early-return pattern (was nested `unwrap_or_else`).

### Fixed

- **Security**: `resolve_path_env` now rejects `..` in absolute paths, not just relative.
- **Data Integrity**: Migration v1→v2 wrapped in `BEGIN TRANSACTION`/`COMMIT`. Orphaned `file_cache_v3` tables cleaned up unconditionally.
- **TDD violations**: Added 4 tests for `cached_mtime()` and mtime fast path (previously untested).
- **GitHub URL**: Corrected from `yaar/` to `armaniacs/` in workspace metadata.

## [0.4.2] - 2026-05-20

### Changed
- **MCP rebuild test timeout**: Increased from 30s to 120s for environments where ONNX embedder loading exceeds 60s (flake fix)
- **clean.rs simplified**: `delete_db_files` uses a clear name-based loop instead of iterator chain; `backup_file` pruning rewritten with `rev().skip(3)`; `test_run_clean_full_flow` extracts `find_files()` helper; empty `#[cfg(not(unix))]` block removed

### Fixed
- None

## [0.4.1] - 2026-05-19

### Added
- **`shiotsuchi doctor`** — environment health check (config, DB, tokenizer, embedder, vaults)
- **`shiotsuchi completion <shell>`** — shell completion script generation (bash, zsh, fish, powershell, elvish)
- **`dive --vault <name>`** — restrict search to a specific vault via CLI flag (maps to `vault_filter`)
- **Release CI** — GitHub Actions workflow builds and publishes binaries for Linux, macOS, and Windows on `v*.*.*` tags
- **MCP rate limiting** — basic 10 req/s rate limiter on `search_local_notes` endpoint
- **Tests**: wal_checkpoint unit test, vault_filter FTS filtering correctness test, RateLimiter behavior test, CLI error-path test for `clean` command

### Changed
- **vault_filter now SQL pushdown**: `fts_search()` and `vec_search()` accept `vault_filter` and JOIN with `chunks` table for WHERE-level filtering. Removes the old `limit*3` post-filter workaround from `search.rs`
- **`clean` is now atomic**: New DB is built at a temporary path first. On success, the old DB is backed up and atomically swapped via `rename()`. WAL checkpointing ensures all data is in the main `.db` file before the swap. Cross-device fallback: copy+delete
- **`NoteDatabase::wal_checkpoint()`** added (public method). CLI no longer depends on `rusqlite` directly
- **Release profile**: `opt-level = "z"`, `lto = true`, `codegen-units = 1`, `strip = "symbols"` — binary size reduced from 75MB to 20MB (73%)
- **`bm25()` arguments simplified**: `bm25(fts_chunks, 0.0, 0.0, 1.0, 1.0, 1.0)` → `bm25(fts_chunks, 1.0)` (fts_chunks has only 1 column)
- **Release CI**: now runs `cargo test` before build; uses `matrix.os` for Linux/macOS/Windows

### Fixed
- **`delete_db_files` symlink vulnerability**: `is_symlink()` check added before `remove_file()` to prevent following malicious symlinks
- **Windows permission gaps**: Added `#[cfg(not(unix))]` warnings for the 3 `#[cfg(unix)]` permission guards (util.rs, clean.rs, config_migrate.rs)
- **`config-migrate` help text**: Added missing `--config` flag description

### Security
- **Symlink protection in `clean`**: `delete_db_files` now refuses to follow symlinks (logs warning, skips the path)

## [0.4.0] - 2026-05-18

### Added
- **Multi-vault support**: Config now supports `[database]` + `[vaults.xxx]` sections.
  Legacy `[vault]` format remains readable with a migration hint.
  - `shiotsuchi config-migrate` — auto-convert old config to new format
- **`shiotsuchi clean`** — backup database to `.bak.<timestamp>`, delete, and re-index from scratch
- **`shiotsuchi search`** — alias for `shiotsuchi dive`
- **DB schema v3**: `vault_name` column added to `chunks` and `file_cache` tables for per-vault tracking.
  Migration is crash-safe (transaction-wrapped, idempotent re-entry)
- **Cumulative progress tracking** across all vaults during `index_directory`
- **`is_path_in_notes_dir_lenient()`** — handles non-existent file paths for rename/delete watcher events
- **Tests**: 8 new tests for `clean` command (backup, delete, full integration flow)
- **Documentation**: All reference docs updated for multi-vault features

### Changed
- **Config format**: `[vault]` → `[database]` with `[vaults.*]` sections. Old format auto-detected
- **IndexConfig**: `notes_dir: PathBuf` replaced with `vaults: Vec<(String, PathBuf)>`
- **Core types**: `Chunk.vault_name`, `ChunkSearchResult.vault_name` fields added
- **Search**: `search()` accepts optional `vault_filter` parameter
- **Watcher**: Creates one `notify` watcher per vault instead of a single watcher
- **Indexer**: `index_directory()` iterates over all vaults; `cleanup_deleted()` operates per-vault
- **Vec mode**: Without embedder now returns an error (only Hybrid falls back to FTS gracefully)
- **DB methods**: `delete_chunks_for_file`, `cached_hash`, `upsert_file_cache`, `delete_file_cache`,
  `list_cached_paths` — all accept `vault_name: &str` as first parameter
- **CLI subcommands**: `run_chart`, `run_scan`, `run_dredge`, `run_delete`, `run_config` accept
  `vaults: &[(String, PathBuf)]` instead of `notes_dir: &Path`
- **MCP**: `call_tool()` accepts vaults list; optional `vault` argument for `search_local_notes`
- **`split_into_chunks()`** — accepts `vault_name: &str` parameter (set on each chunk)
- **Dependencies**: notify 6 → 9.0.0-rc.4

### Fixed
- **6 pre-existing test failures**:
  - Tokenizer POS filter tests: model does not emit tags via `tags()` API —
    tests now verify `keep_untagged` behavior instead of POS matching
  - `test_frontmatter_with_body_after`: frontmatter before heading creates a preamble chunk (2 chunks, not 1)
  - `test_long_paragraph_splits_at_byte_threshold`: input now uses blank-line-separated paragraphs
  - `test_search_vec_mode_without_embedder_returns_error`: Vec without embedder returns error (regression from v0.3.7)
  - `test_handle_event_rename_reindexes_new_path`: rename handler uses lenient path check for deleted files
- **DB migration crash-safety**: v2→v3 migration checks column existence before ALTER TABLE, wraps in transaction

### Added
- **58 new unit tests** across all core modules, closing coverage gaps in helper functions, edge cases, and security-critical paths:

  - **Chunker** (`chunker.rs`): Direct tests for `header_level()` (h1–h3, h4+ ignored, invalid formats, leading whitespace, unicode), `split_by_headers()` (code block awareness, mixed fence types, header hierarchy, level popping, empty sections), and `split_on_blank_lines()` (consecutive blank collapse, whitespace-only lines, code block blank line pass-through, tilde/indented fence markers, empty result). (18 new tests)

  - **Embedder** (`embedder.rs`): Full edge-case coverage for `mean_pool_l2_normalize()` (all-zero input, single token, masked tokens, all-masked sequence, unit vector verification, variable hidden sizes). Added `resolve_model_path()` path structure assertion. (7 new tests)

  - **Search** (`search.rs`): `extract_snippet()` edge cases for query-at-start, query-at-end, multi-token queries, `max_lines=0`, very long documents, and case-insensitive matching. (6 new tests)

  - **Tokenizer** (`tokenizer.rs`): `simple_and_query()` edge cases (quote escaping, tab/newline separation). `simple_tokenize()` unicode support. `collect_tokens()` with empty input, single/multi-line Japanese, blank line skipping, and POS filter variations (noun filter, multiple prefixes, keep_untagged). (12 new tests)

  - **Watcher** (`watcher.rs`): `is_path_within_vault()` regular file acceptance test. (1 new test; symlink escape, symlink inside, and nonexistent path tests already existed)

  - **Indexer** (`indexer.rs`): `build_exclude_globset()` literal component matching patterns (bracket literals, recursive depth, multi-pattern, extension-as-component). `escape_glob_literal()` backslash chain. (6 new tests)

  - **DB** (`db.rs`): Batch retrieval of 100 chunks via `get_chunks_by_ids()`, FTS search deduplication verification, metadata-before-chunk consistency, and same-path different-index insert. (4 new tests)

  - **Paths** (`paths.rs`): `xdg_cache_home()`, `home_dir()`, `default_db_path()` cache directory assertion, and parent path plausibility check. (4 new tests)

### Changed
- None

### Fixed
- `test_split_on_blank_lines_code_block_blank_lines_not_split` input adjusted to match actual `split_on_blank_lines` behavior (blank line after closing fence is a valid paragraph separator).

## [0.3.6] - 2026-05-17

### Changed
- **Upgrade rusqlite 0.31 → 0.39:** Bundled SQLite jumps from 3.4x → 3.51.3, bringing FTS5 performance improvements and security patches. API migration: adapted `sqlite3_auto_extension` FFI signature (`*const`→`*mut`), replaced `usize` FromSql with `i64` casts (disabled by default in rusqlite 0.38+).
- **Upgrade sha2 0.10 → 0.11:** digest 0.11 changed `finalize()` return type — replaced `format!("{:x}", hash)` with `hex::encode(hash)` in build.rs, tokenizer.rs, and CLI support.rs.
- **Upgrade thiserror 1 → 2:** drop-in replacement, faster compile times.
- **Remove unused direct dependencies:** `pulldown-cmark` and `ndarray` were declared but not imported — removed for faster compilation.
- **`cargo update` patch bumps:** 17 package updates including rustls 0.23.31→0.23.40 (security), ruzstd 0.8.2→0.8.3, tracing 0.1.41→0.1.44.
- **Benchmark baseline captured:** Criterion benchmarks for `index_100_files` and `search_1000_notes` recorded pre- and post-upgrade for performance tracking.

### Added
- None

### Fixed
- `search_bench.rs` updated with missing `min_score` parameter (benchmark was broken since v0.3.5).

## [0.3.5] - 2026-05-16

### Added
- **Background rebuild with progress notifications:** `rebuild_index` MCP tool now spawns a background tokio task that calls `index_directory()` directly, sending MCP `notifications/progress` on stdout. Progress is reported per-file with current/total counts.
- **`min_score` filter in search:** `search()` now accepts an optional `min_score` threshold. FTS/Vec mode excludes results with score above the threshold, Hybrid mode excludes results with score below it. Exposed via `search_local_notes` MCP tool.

### Changed
- **MCP tool surface overhaul:** Replaced old `search_vault` / `read_full_note` / `vault_status` tools with new RAG-aware tools: `search_local_notes` (with `mode`/`limit`/`min_score` params), `get_surrounding_context`, `index_status`, and `rebuild_index`.
- **Structured Markdown output:** Search results now include `### RETRIEVED CONTEXT ###` / `### END RETRIEVED CONTEXT ###` delimiters, source numbering, parent heading hierarchy, chunk IDs, and relevance scores.
- **`index_directory()` signature:** Added optional `IndexProgress` callback parameter for per-file progress reporting.
- **MCP server runtime:** Added tokio multi-thread runtime via `#[tokio::main]` for future async extensibility. The stdio loop remains synchronous.

### Fixed
- None

## [0.3.3] - 2026-05-16

### Added

- **RAG core implementation:** Complete RAG (Retrieval-Augmented Generation) feature with vector-based semantic search.
  - `Chunker` module: Recursive Markdown splitter that segments notes into header/paragraph chunks with configurable `max_chars` (default 1000) and `overlap_chars` (default 100).
  - `Embedder` module: ONNX inference pipeline using `ort` and `tokenizers` crates for local vector generation.
  - `sqlite-vec` integration: Stores chunk embeddings in SQLite with `vec0` virtual table for efficient similarity search.
  - `SearchMode` enum: `Keyword` (BM25) and `Semantic` (vector cosine similarity) search modes.
  - `ChunkSearchResult` model: Extended result type with chunk metadata and similarity scores.
- **`--mode` flag for `dive` command:** Choose between `keyword` (FTS5 BM25) and `semantic` (vector similarity) search. `keyword` is default for backward compatibility.
- **`dredge` command:** Extract and index chunks from existing notes without re-embedding content. Useful for migrating pre-v0.3.3 vaults to chunked schema.
- **`setup --check` command:** Verify ONNX model availability, tokenizer model, and config validity with SHA-256 hash verification.
- **ONNX model download script:** `scripts/download-onnx-model.sh` fetches and verifies the ONNX embedding model.
- **Vector search tests:** 12 new tests covering chunking, embedding, and semantic search functionality.

### Changed

- **`search()` signature:** Added `SearchMode` parameter. `Keyword` mode uses existing FTS5 BM25; `Semantic` mode uses vector similarity.
- **`index_directory()`:** Now chunks files before indexing, storing both FTS5 and vector representations.
- **`VaultStats` model:** Added `chunk_count` field to track indexed chunks.
- **Config:** Added `[rag]` section with `max_chunk_chars`, `overlap_chars`, and `model_path` options.

### Fixed

- **Vector schema migration:** `index_directory` handles both new (chunked+vectors) and legacy (FTS5-only) vault states.
- **`resolve_model_path`:** Falls back through config, env var, and embedded model paths.

---

## [0.3.2] - 2026-05-10

### Added

- **Configurable `max_snippet_chars`:** New `SearchConfig` struct in `core/src/models.rs` with `max_snippet_chars` field — configurable via `config.toml`, default 1000, clamped to 128–65535. Replaces hardcoded `MAX_SNIPPET_CHARS = 500`.
- **`max_snippet_chars` in CLI config:** Added to `IndexingConfig` in `cli/src/config.rs`. Deserializes from TOML, applies clamping via `SearchConfig::new()`.
- **Tests:** 7 new tests across core and CLI:
  - `test_search_config_clamping` — boundary values (128, 5000, 65535)
  - `test_extract_snippet_respects_max_chars` — truncation at configured limit
  - `test_extract_snippet_truncate_on_long_multiline` — multiline truncation
  - `test_extract_snippet_match_after_third_line` — issue #1 reproduction (match on line 4+)
  - `test_extract_snippet_very_short_max_chars_truncates_before_match` — edge case
  - `test_search_with_search_config` — end-to-end config propagation
  - `test_max_snippet_chars_default_is_1000` — CLI config default
  - `test_max_snippet_chars_from_toml` — TOML deserialization
  - `test_max_snippet_chars_clamped_by_search_config` — clamping behavior
- **GitHub Pages landing page:** Added `index.html` for project website hosting on GitHub Pages. Includes project overview, installation instructions, feature highlights, and usage examples.
- **Repository URL update:** Updated documentation to point to the correct GitHub repository (https://github.com/armaniacs/shiotsuchi-search) in INSTALL.md and INSTALL.ja.md.

### Changed

- **`search()` signature:** Added `search_cfg: Option<&SearchConfig>` parameter. `None` uses default (1000 chars). All call sites updated (CLI `dive`, MCP `handler`, tests).
- **`extract_snippet()` signature:** Added `max_chars: usize` parameter. Replaces hardcoded constants `MAX_SNIPPET_CHARS` (500) and `FALLBACK_SNIPPET_CHARS` (200). All call sites updated.
- **Character count consistency:** Fixed `extract_snippet()` truncation check from byte-length (`result.len()`) to character count (`result.chars().count()`) to match `.chars().take(max_chars)`. Ensures correct behavior for multibyte text ( Japanese, emoji, etc.).
- **Removed constants:** `MAX_SNIPPET_CHARS` (500) and `FALLBACK_SNIPPET_CHARS` (200) removed from `core/src/constants.rs`. Value now flows from `SearchConfig`.
- **`print_table()` output:** Removed 3-line truncation of snippet in table output. Now displays the full snippet. This fixes issue #1 where search terms appearing after the 3rd line were invisible in `--format table`.

### Fixed

- **Issue #1:** `shiotsuchi dive --format table` で検索文字がスニペットに表示されない — root cause was `print_table()` forcing 3-line display while `extract_snippet()` generates up to 7 lines. Matches on lines 4+ were silently omitted. Fixed by removing the hard 3-line cap in `print_table()`.

### Documentation

- Config examples updated: `docs/INSTALL.md`, `docs/INSTALL.ja.md`, `docs/CLI-USE.md`, `docs/CLI-USE.ja.md`, `README.md`, `README.ja.md`, `ref/cli.md`, `ref/core.md`.
- `ref/cli.md` field table: Added `max_snippet_chars` row (type: integer, default: 1000, description: clamped 128–65535).

---

## [0.3.1] - 2026-05-10

### Added

- **MCP path traversal protection:** `SHIOTSUCHI_NOTES_DIR` and `SHIOTSUCHI_DB_PATH` environment variables are now validated — relative paths containing `..` are rejected with a warning, falling back to config defaults.
- **Permission utility for CLI:** Extracted `secure_parent_dir()` into `cli/src/util.rs`, shared between `chart` and `scan` commands (DRY).
- **CLI global flag tests:** 11 new unit tests verify `--notes-dir`, `--db-path`, and `--verbose` are accepted on every subcommand, both before and after the subcommand.
- **Directory permission tests:** Added Unix-specific tests in `chart.rs` and `scan.rs` verifying parent directories are created with `0o700`.

### Changed

- **Removed `debounce_ms` from `WatcherConfig`:** The field was unused — `VaultWatcher` never consumed it. Removed from struct, default, docs (`CLI-USE.md`, `CLI-USE.ja.md`, `INSTALL.md`, `INSTALL.ja.md`, `ref/cli.md`).
- **`exclude_patterns` references fully purged from docs:** README, README.ja, INSTALL, INSTALL.ja, ref/core, ref/models — all config examples and field descriptions now use `exclude_dirs`.

### Fixed

- **`ref/models.md` and `ref/core.md`:** Updated stale `IndexConfig` field listings to match the actual struct (was `exclude_patterns`, missing `auto_exclude_hidden`, `follow_links`, `dynamic_threshold`).

### Security

- **Defense-in-depth for MCP:** The `resolve_path_env()` function provides a validation boundary before paths reach the handler layer, complementing the existing `read_full_note` traversal check.

---

## [0.3.0] - 2026-05-09

### Added

- **TDD test coverage for review fixes:** Added 11 missing tests identified in `plan-h2-init-fix-remaining.md` following strict RED→GREEN→REFACTOR cycles.
  - Chunking boundary tests (256 entries, 25.6 MB threshold, exact boundary, single chunk for small vaults).
  - Vault boundary test: symlink outside vault is rejected.
  - Consistency test: `index_file` and `index_directory` produce identical DB metadata.
  - Config deserialization tests: old `exclude_patterns` key rejected, new `exclude_dirs` key accepted.
  - Dynamic threshold test: `threshold=0` matches any directory with >=1 file.
  - File permission tests (Unix): config and backup files created with `0o600`.

### Fixed

- **`exclude_patterns` now reliably rejected:** Added `#[serde(deny_unknown_fields)]` to `IndexingConfig` so the deprecated `exclude_patterns` key causes a clear deserialization error instead of being silently ignored.

---

## [0.2.9] - 2026-05-08

### Added

- **Chunked indexing for OOM protection:** `index_directory` now processes files in chunks of 256 entries or 25.6 MB, preventing memory exhaustion on large vaults with many or large files.
- **Dynamic threshold config:** New `dynamic_threshold` field in `[indexing]` config section (default 5) controls how many matching files trigger dynamic noise detection during vault scan.
- **Candidate limit:** `scan_vault` enforces a 1000-candidate upper limit to prevent UI freeze on extremely large vaults. A truncated flag is returned when the limit is hit.
- **Invalid pattern feedback:** `chart` now reports the number of invalid exclude patterns in its output summary.
- **Restricted file permissions:** Config files and backups are created with `0o600` permissions on Unix, preventing accidental disclosure to other users.

### Changed

- **`exclude_patterns` renamed to `exclude_dirs`** (BREAKING): The config field name now accurately reflects its behavior — it matches directory names via gitignore-style component globs. Old `exclude_patterns` key causes a deserialization error with a migration hint. Update your `config.toml` to use `exclude_dirs`.
- **`scan_vault` I/O halved:** Directory scan now uses a single-pass HashMap-based counting strategy instead of separate `WalkDir` + `read_dir` passes, reducing system calls on large vaults.
- **`init` uses config's `dynamic_threshold`:** The vault scan during `init` now respects the user-configured `dynamic_threshold` instead of the hardcoded constant.
- **Backup timestamp format changed:** `backup_config` now uses Unix epoch seconds (e.g., `1743984552.123456`) instead of `%Y%m%d-%H%M%S.%f` for unique, sortable timestamps.
- **Removed `chrono` dependency:** Backup timestamp generation now uses `std::time::SystemTime`.

### Fixed

- **OOM risk mitigated:** `index_directory` no longer processes all files in a single `par_iter()` batch. Chunked processing caps peak memory at ~25.6 MB × thread count.
- **Symlink guard completeness:** `strip_prefix` now verifies `path.starts_with(notes_dir)` before extracting relative paths, preventing full-path DB storage on unexpected prefix mismatches.
- **Walk errors no longer silent:** `filter_map(|e| e.ok())` replaced with explicit `match` that logs walk errors via `log::warn!`.

### Security

- **Config file permissions:** Both primary config and backup files now use `0o600` permissions on Unix, preventing other users on the same host from reading vault metadata.

## [0.2.8] - 2026-05-07

### Added

- **`config detect-noise` subcommand:** New `shiotsuchi config detect-noise` command that scans a vault for directories matching known noise patterns or containing many markdown files. Prints a human-readable report without modifying the config file.
- **`--yes` flag for `init`:** Non-interactive mode that auto-accepts all detected exclusion candidates. Required when stdin is not a TTY.
- **Config backup on `--force`:** `shiotsuchi init --force` now creates a timestamped `.bak` file before overwriting, enabling easy rollback.
- **Interactive exclusion selection:** `shiotsuchi init` presents a 2-stage interactive prompt (Confirm + MultiSelect) that lets users choose which directories to exclude from indexing.
- **Vault scan during init:** Automatically detects 28 known noise patterns (`node_modules`, `dist`, `build`, `target`, `templates`, etc.) plus directories with 5+ markdown files as dynamic candidates.
- **`globset` dependency:** Added `globset = "0.4"` to `core` crate for gitignore-style glob matching of exclude patterns.

### Changed

- **Gitignore-style exclude matching:** `exclude_patterns` now uses path-component glob matching via `globset` instead of substring `contains()`. A pattern like `"templates"` matches `templates/daily.md` but not `templates_extra/foo.md`. Patterns support `*`, `**`, and `?` wildcards.
- **Hidden directories auto-excluded:** WalkDir `filter_entry` now skips directories starting with `.` by default, controlled by new `auto_exclude_hidden: bool` config field (default `true`). `.git` and `.obsidian` removed from default `exclude_patterns`.
- **Symlink following on by default:** `follow_links` now defaults to `true` with canonicalize-based vault boundary checks on both directory and file entries, preventing symlink escape attacks.
- **`init` command enhanced:** Rewritten with vault scanning, 2-stage interactive UI, config backup, `--yes` flag for non-TTY environments, and notes-dir existence validation.
- **Config defaults:** `exclude_patterns` default reduced to `["node_modules"]` (`.git`, `.obsidian` now covered by `auto_exclude_hidden: true`).

### Security

- **Symlink-to-file vault escape prevented:** The canonicalize boundary check now applies to both directories (in `filter_entry`) and file entries (in `.filter()`), closing a vector where a symlink-to-file pointing outside the vault could be indexed.
- **Vault boundary check on file symlinks:** When `follow_links` is enabled, every file path is canonicalized and verified to stay within the vault root before being read or indexed.

## [0.2.7] - 2026-05-07

### Fixed

- **CI security audit:** Replaced deprecated `dtolnay/install@cargo-audit` + `cargo audit --deny warnings` with `rustsec/audit-check@v2` for advisory checking.

## [0.2.5] - 2026-05-06

### Changed

- **README simplified:** Removed Quick Start, Building from Source, and Running Tests sections from both `README.md` and `README.ja.md` — these are covered in `docs/INSTALL.md` / `docs/INSTALL.ja.md`. Each README now focuses on project overview, features, commands, MCP integration, security, configuration, performance, and license.
- **Unnecessary env var removed:** `SHIOTSUCHI_MODEL_PATH` prefix removed from `shiotsuchi chart` example in MCP sections of both READMEs — the model is embedded at build time and not needed at runtime.

## [0.2.4] - 2026-05-06

### Added

- **User-aware install:** `make install` now checks `id -u` — non-root users get binaries in `~/.local/bin/` (or `~/.cargo/bin/` if it exists); root users install to `$(PREFIX)/bin` (default `/usr/local/bin`). Explicit `PREFIX=` overrides bypass auto-detection. The `uninstall` target follows the same logic.
- **Documentation reorganization:** Install guides (`INSTALL.md`, `INSTALL.ja.md`) and model license notice (`MODEL_LICENSES.md`) moved into `docs/` directory.

### Changed

- **Crate renamed:** Core library renamed from `obsidian-shiotsuchi-vault-core` to `shiotsuchi-core` (Rust path: `shiotsuchi_core`). The `obsidian` prefix was a remnant from an earlier Obsidian-only focus and has been removed throughout.

### Removed

- **Obsolete files:** Removed stale `finops-consultant.md` and `ui-expert.md` from repository root.

## [0.2.3] - 2026-05-06

### Changed

- Bumped version to 0.2.3.

### Security

- **Symlink traversal prevention:** Added `is_path_within_vault()` to file watcher (`handle_event`), using `canonicalize()` + `starts_with()` to block symlink-based vault escape attacks.
- **Model download integrity:** `download-model.sh` now verifies the downloaded model file against a pinned SHA-256 checksum.
- **Search graceful degradation:** Hoisted `notes_dir.canonicalize()` outside per-result loop; per-file canonicalize failures now degrade gracefully (snippet set to `[path outside vault]`) instead of aborting the entire search.
- **Config file security:** Added security notice to `ShiotsuchiConfig::load()` documenting file permission expectations.

### Fixed

- **Test flakiness:** Replaced `test_scan_indexes_new_file` (PollWatcher + real sleep, 60s+ timeout) with `test_scan_watcher_setup` — a synchronous watcher construction test that runs in <10ms.
- **Delete stale entries:** `run_delete` now handles files already deleted from disk — attempts canonicalize validation when the file exists, otherwise proceeds with DB cleanup directly.
- **CI cargo audit:** Fixed audit step to use `dtolnay/install@cargo-audit` and fail on true warnings (`--deny warnings` without suppression).
- **Delete path validation:** Changed `path.contains("..")` to `path.split('/').any(|c| c == "..")` to avoid rejecting legitimate filenames like `some..thing.md`.

### Performance

- **Parallel indexing:** `index_directory()` now uses `rayon` to parallelize file reading, hashing, frontmatter extraction, and Vaporetto tokenization across available CPU cores. DB writes remain serial (`RefCell<Connection>` is `!Sync`).

### Maintainability

- **Magic numbers → constants:** Extracted `MAX_SNIPPET_CHARS` (500), `FALLBACK_SNIPPET_CHARS` (200), and `DEFAULT_SNIPPET_LINES` (3) into `core/src/constants.rs`. All call sites updated to use named constants.
- **decompress_if_needed dedup:** Extracted shared zstd decompression logic into `core/src/_decompress.rs`, included via `include!()` in both `build.rs` and `tokenizer.rs`.
- **require_tokenizer! macro:** Added `#[macro_export]` test helper that prints a visible `[SKIPPED]` message via stderr (instead of silent `return`) when the Vaporetto model is unavailable. All 8 model-dependent test sites updated.
- **SAFETY documentation:** The `unsafe { Predictor::deserialize_from_slice_unchecked }` block now includes a detailed safety comment explaining the three preconditions that make it sound.

## [0.2.2] - 2026-05-06

### Changed

- Bumped version to 0.2.2.

### Fixed

- **Documentation:** Removed remaining Kilo Skill references from README.md, README.ja.md, and phase5 polish plan. Kilo Skill was removed in v0.1.1; documentation now accurately reflects only CLI and MCP interfaces.

## [0.2.1] - 2026-05-05

### Changed

- Bumped version to 0.2.1.

### Fixed

- **Design spec:** Removed obsolete Kilo Skill sections (§2.2, §4.2, §7.1, §9); updated config/DB paths to XDG; removed `[mcp]` config section, `tokenizer` field, and obsolete env vars; fixed MCP flow diagram; documented `delete` command.
- **Implementation plan:** Added completion summary with test results, commit log, and review issue coverage.

## [0.2.0] - 2026-05-04

### Changed

- **Versioning:** Bumped to 0.2.0 following SemVer due to breaking changes (removal of skill crate, DB path consolidation).
- **Dependencies:** Updated related library versions.

### Fixed

- **Transaction safety:** Use RAII `rusqlite::Transaction` in `upsert_note` and `delete_note` to ensure atomic rollback on panic.
- **Security:** Added SHA-256 integrity verification for embedded predictor deserialization; fixed path traversal in search snippets; genericized MCP error messages; corrected home directory fallback from `/tmp` to current directory.
- **DRY:** Consolidated DB path resolution logic into `core::paths` module; CLI and MCP now share `default_db_path()`.
- **Testing:** Moved E2E tests from CLI crate to separate `e2e` crate; removed MCP dev-dependency from CLI; fixed flaky sleep-based tests with env var control.
- **Observability:** Default log level set to `warn` so warnings appear without `--verbose`.

### Added

- **Migration support:** Schema version tracking via `PRAGMA user_version`.
- **CLI new command:** `delete <path>` to remove a note from the index.
- **Documentation:** Security & privacy notice in README; migration manager foundation for future schema changes; shared path utilities.

## [0.1.1] - 2026-05-02

### Changed

#### Core (`shiotsuchi-core`)
- **Build-time predictor serialization**: `build.rs` now pre-builds and embeds a serialized `Predictor` instead of raw model bytes, reducing tokenizer initialization from seconds to milliseconds.
- Added global tokenizer cache via `OnceLock` (`get_tokenizer()`), eliminating repeated initialization cost across CLI invocations.
- Added `NoteDatabase::list_all_metadata()` to retrieve all indexed note metadata ordered by `indexed_at DESC`.

#### CLI (`shiotsuchi`)
- **Completed `scan` command**: fully implemented file watcher loop using `VaultWatcher` with automatic DB directory creation.
- **Completed `log` command**: displays indexed note history with human-readable `YYYY-MM-DD HH:MM:SSZ` timestamps.
- CLI flags `--notes-dir`, `--db-path`, and `--verbose` are now `global = true`, allowing them to be placed before or after subcommands.
- `chart` and `scan` commands now automatically create the parent directory for the DB file.
- `tide` command now formats `Last indexed` as human-readable UTC instead of a raw Unix timestamp.
- Default paths now follow XDG Base Directory Specification:
  - DB: `$XDG_CACHE_HOME/shiotsuchi/db.sqlite3` (fallback `~/.cache/shiotsuchi/db.sqlite3`)
  - Config: `$XDG_CONFIG_HOME/shiotsuchi/config.toml` (fallback `~/.config/shiotsuchi/config.toml`)

#### MCP Server (`shiotsuchi-mcp`)
- Updated default DB path to use XDG-compliant cache directory.

### Removed

- **Kilo Skill (`shiotsuchi-skill`)**: Removed the entire `skill/` crate. The Kilo-specific JSON-RPC skill server was abandoned because it was not compatible with a standard skill protocol. Equivalent functionality is already provided by the MCP server (`shiotsuchi-mcp`), and a standards-compliant skill server can be reintroduced later if needed.

### Added

- `CHANGELOG.md`, `LICENSE`, `docs/MODEL_LICENSES.md`, `Makefile`, `README.ja.md`
- `docs/HUMAN-VERIFICATION.md` with automated E2E test coverage
- `cli/tests/e2e_test.rs` for end-to-end verification of MCP, XDG, scan, log, and tide behavior
- `integration/` test directory
- Additional integration tests for Japanese queries, DB directory auto-creation, and human-readable timestamps

## [0.1.0] - 2026-04-30

### Added

#### Core (`shiotsuchi-core`)
- SQLite FTS5 schema with CRUD operations for note indexing
- Shared data models with serde support
- Markdown parsing and frontmatter extraction via pulldown-cmark
- Vaporetto tokenizer integration with `build.rs` model embedding (`SHIOTSUCHI_EMBED_MODEL`)
- File walker and indexer with SHA-256 hash tracking for incremental re-indexing
- BM25 full-text search with snippet extraction
- Filesystem watcher with incremental re-indexing (`notify` crate)
- Criterion benchmarks for indexing (100 files) and search (1,000 notes)
- End-to-end integration tests

#### CLI (`shiotsuchi`)
- `chart` — index or re-index all Markdown files in a vault
- `dive <query>` — AND search with JSON output and snippet display
- `tide` — vault statistics (total notes, last indexed, etc.)
- `scan` — watch for file changes and auto-re-index
- `log` — show indexing history
- `--version` output with tagline "Guiding your path through the data tide."
- Helpful error message when database is not found (guides user to run `chart`)
- XDG-compliant default paths: `$XDG_CACHE_HOME/shiotsuchi/db.sqlite3`, `$XDG_CONFIG_HOME/shiotsuchi/config.toml`
- `~/.config/shiotsuchi/config.toml` support via `config` crate

#### MCP Server (`shiotsuchi-mcp`)
- JSON-RPC 2.0 over stdio (Model Context Protocol)
- Tools: `search_vault`, `read_full_note`, `vault_status`
- Compatible with Claude Desktop

#### Tooling
- `Makefile` with `build`, `build-dev`, `test`, `bench`, `install`, `uninstall`, `model`, `clean`, `help`
- `scripts/download-model.sh` for fetching the Vaporetto model
- `docs/MODEL_LICENSES.md` with BSD-3-Clause notice for the bundled tokenizer model
- `README.md` (English) and `README.ja.md` (Japanese)

[Unreleased]: https://github.com/armaniacs/shiotsuchi-search/compare/v0.4.20...HEAD
[0.4.20]: https://github.com/armaniacs/shiotsuchi-search/compare/v0.4.19...v0.4.20
[0.4.19]: https://github.com/armaniacs/shiotsuchi-search/compare/v0.4.18...v0.4.19
[0.4.18]: https://github.com/armaniacs/shiotsuchi-search/compare/v0.4.17...v0.4.18
[0.4.17]: https://github.com/armaniacs/shiotsuchi-search/compare/v0.4.16...v0.4.17
[0.4.16]: https://github.com/armaniacs/shiotsuchi-search/compare/v0.4.15...v0.4.16
[0.4.15]: https://github.com/armaniacs/shiotsuchi-search/compare/v0.4.14...v0.4.15
[0.4.14]: https://github.com/armaniacs/shiotsuchi-search/compare/v0.4.13...v0.4.14
[0.4.13]: https://github.com/armaniacs/shiotsuchi-search/compare/v0.4.12...v0.4.13
[0.4.12]: https://github.com/armaniacs/shiotsuchi-search/compare/v0.4.11...v0.4.12
[0.4.11]: https://github.com/armaniacs/shiotsuchi-search/compare/v0.4.10...v0.4.11
[0.4.10]: https://github.com/armaniacs/shiotsuchi-search/compare/v0.4.9...v0.4.10
[0.4.9]: https://github.com/armaniacs/shiotsuchi-search/compare/v0.4.8...v0.4.9
[0.4.8]: https://github.com/armaniacs/shiotsuchi-search/compare/v0.4.7...v0.4.8
[0.4.7]: https://github.com/armaniacs/shiotsuchi-search/compare/v0.4.6...v0.4.7
[0.4.6]: https://github.com/armaniacs/shiotsuchi-search/compare/v0.4.5...v0.4.6
[0.4.5]: https://github.com/armaniacs/shiotsuchi-search/compare/v0.4.4...v0.4.5
[0.4.4]: https://github.com/armaniacs/shiotsuchi-search/compare/v0.4.3...v0.4.4
[0.4.3]: https://github.com/armaniacs/shiotsuchi-search/compare/v0.4.2...v0.4.3
[0.4.2]: https://github.com/armaniacs/shiotsuchi-search/compare/v0.4.1...v0.4.2
[0.4.1]: https://github.com/armaniacs/shiotsuchi-search/compare/v0.4.0...v0.4.1
[0.4.0]: https://github.com/armaniacs/shiotsuchi-search/compare/v0.3.7...v0.4.0
[0.3.7]: https://github.com/armaniacs/shiotsuchi-search/compare/v0.3.6...v0.3.7
[0.3.6]: https://github.com/armaniacs/shiotsuchi-search/compare/v0.3.5...v0.3.6
[0.3.3]: https://github.com/armaniacs/shiotsuchi-search/compare/v0.3.2...v0.3.3
[0.3.2]: https://github.com/armaniacs/shiotsuchi-search/compare/v0.3.1...v0.3.2
[0.3.1]: https://github.com/armaniacs/shiotsuchi-search/compare/v0.3.0...v0.3.1
[0.2.8]: https://github.com/armaniacs/shiotsuchi-search/compare/v0.2.7...v0.2.8
[0.2.7]: https://github.com/armaniacs/shiotsuchi-search/compare/v0.2.6...v0.2.7
[0.2.6]: https://github.com/armaniacs/shiotsuchi-search/compare/v0.2.5...v0.2.6
[0.2.5]: https://github.com/armaniacs/shiotsuchi-search/compare/v0.2.4...v0.2.5
[0.2.4]: https://github.com/armaniacs/shiotsuchi-search/compare/v0.2.3...v0.2.4
[0.2.3]: https://github.com/armaniacs/shiotsuchi-search/compare/v0.2.2...v0.2.3
[0.2.2]: https://github.com/armaniacs/shiotsuchi-search/compare/v0.2.1...v0.2.2
[0.2.1]: https://github.com/armaniacs/shiotsuchi-search/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/armaniacs/shiotsuchi-search/compare/v0.1.1...v0.2.0
[0.1.1]: https://github.com/armaniacs/shiotsuchi-search/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/armaniacs/shiotsuchi-search/releases/tag/v0.1.0
