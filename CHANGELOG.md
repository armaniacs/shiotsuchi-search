# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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

#### Core (`obsidian-shiotsuchi-vault-core`)
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

- `CHANGELOG.md`, `LICENSE`, `MODEL_LICENSES.md`, `Makefile`, `README.ja.md`
- `docs/HUMAN-VERIFICATION.md` with automated E2E test coverage
- `cli/tests/e2e_test.rs` for end-to-end verification of MCP, XDG, scan, log, and tide behavior
- `integration/` test directory
- Additional integration tests for Japanese queries, DB directory auto-creation, and human-readable timestamps

## [0.1.0] - 2026-04-30

### Added

#### Core (`obsidian-shiotsuchi-vault-core`)
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

#### Kilo Skill (`shiotsuchi-skill`)
- JSON-RPC stdio server exposing `search-vault`, `read-note`, `vault-status`
- `skill.yaml` manifest

#### MCP Server (`shiotsuchi-mcp`)
- JSON-RPC 2.0 over stdio (Model Context Protocol)
- Tools: `search_vault`, `read_full_note`, `vault_status`
- Compatible with Claude Desktop

#### Tooling
- `Makefile` with `build`, `build-dev`, `test`, `bench`, `install`, `uninstall`, `model`, `clean`, `help`
- `scripts/download-model.sh` for fetching the Vaporetto model
- `MODEL_LICENSES.md` with BSD-3-Clause notice for the bundled tokenizer model
- `README.md` (English) and `README.ja.md` (Japanese)

[Unreleased]: https://github.com/your-org/shiotsuchi-search/compare/v0.1.1...HEAD
[0.1.1]: https://github.com/your-org/shiotsuchi-search/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/your-org/shiotsuchi-search/releases/tag/v0.1.0
