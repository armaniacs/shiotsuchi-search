# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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

[Unreleased]: https://github.com/your-org/shiotsuchi-search/compare/v0.2.5...HEAD
[0.2.5]: https://github.com/your-org/shiotsuchi-search/compare/v0.2.4...v0.2.5
[0.2.4]: https://github.com/your-org/shiotsuchi-search/compare/v0.2.3...v0.2.4
[0.2.3]: https://github.com/your-org/shiotsuchi-search/compare/v0.2.2...v0.2.3
[0.2.2]: https://github.com/your-org/shiotsuchi-search/compare/v0.2.1...v0.2.2
[0.2.1]: https://github.com/your-org/shiotsuchi-search/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/your-org/shiotsuchi-search/compare/v0.1.1...v0.2.0
[0.1.1]: https://github.com/your-org/shiotsuchi-search/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/your-org/shiotsuchi-search/releases/tag/v0.1.0
