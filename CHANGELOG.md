# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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

[Unreleased]: https://github.com/armaniacs/shiotsuchi-search/compare/v0.3.3...HEAD
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
