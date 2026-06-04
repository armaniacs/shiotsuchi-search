# Contributing to Shiotsuchi Search

Thank you for your interest in contributing to Shiotsuchi Search! This guide helps human
developers set up the project, follow the development workflow, and understand important
security and dependency policies.

## Project Overview

Shiotsuchi Search is a high-performance Japanese-aware search engine for Markdown note vaults
(Obsidian, etc.), powered by Vaporetto tokenization and SQLite FTS5. It provides sub-second
full-text search across thousands of notes, with incremental indexing, CLI and MCP (Claude
Desktop) interfaces, and optional semantic search via ONNX Runtime embeddings.

## Prerequisites

- Rust 2021 edition toolchain (see `rust-toolchain.toml`)
- The Vaporetto tokenizer model file (`models/bccwj-suw+unidic_pos+kana.model.zst`)

## Setup

1. Clone the repository:

   ```bash
   git clone https://github.com/armaniacs/shiotsuchi-search.git
   cd shiotsuchi-search
   ```

2. Download the tokenizer model:

   ```bash
   ./scripts/download-model.sh
   ```

3. Build:

   ```bash
   cargo build
   ```

4. Run tests:

   ```bash
   cargo test -p shiotsuchi-core
   ```

## Quick Start

```bash
# Initialize a database (optional interactive setup)
shiotsuchi init

# Index all Markdown files in a notes directory
SHIOTSUCHI_MODEL_PATH=models/bccwj-suw+unidic_pos+kana.model.zst \
  shiotsuchi chart --notes-dir ~/Notes

# Search notes
shiotsuchi search "project plan"
```

### Development Quick Commands

| Command | Description |
|---------|-------------|
| `make build` | Build release binaries (embeds model) |
| `make build-dev` | Build dev profile (no model embedding) |
| `make test` | Run all workspace tests |
| `cargo test -p shiotsuchi-core` | Run core crate tests only |
| `make bench` | Run criterion benchmarks |
| `make doc` | Generate and open local docs |

## Development Workflow

### Branch Naming

Use `<type>/<slug>` format:

- `feat/new-feature`
- `fix/bug-name`
- `docs/update-readme`
- `refactor/simplify-indexer`
- `chore/update-deps`

### Commit Messages

Use [Conventional Commits](https://www.conventionalcommits.org/):

- `feat: add VLM PDF support`
- `fix: correct snippet truncation for CJK`
- `docs: update INSTALL.md`
- `refactor: simplify db transaction logic`
- `chore: upgrade tokio to 1.40`

### TDD Cycle

Red → Green → Refactor. Run tests before committing:

```bash
cargo test -p shiotsuchi-core
```

## VLM Feature Security Notes

> **VLM (Vision-Language Model) support is disabled by default when building from source.**
> It requires the `vlm` feature flag to be explicitly enabled.

### Enabling VLM

```bash
cargo build --features vlm
```

### Security Considerations

- **External API calls**: When VLM is enabled, PDF and image content may be sent to external
  API endpoints (OpenAI, Anthropic, etc.) for visual understanding.
- **API key management**: API keys **MUST** be supplied via environment variables
  (e.g., `OPENAI_API_KEY`). **NEVER** store API keys in `config.toml` or any tracked config
  file.
- **Review before enabling**: Read [`docs/Support-PDF.md`](docs/Support-PDF.md) before enabling
  the `vlm` feature.
- **PR review**: All pull requests that modify or interact with VLM functionality require
  additional security review by the maintainers.

## RC Crate Policy

This project uses Release Candidate (RC) versions of some dependencies. For the current
status and upgrade policy, see [`docs/RC-CRATE-POLICY.md`](docs/RC-CRATE-POLICY.md).

## Related Documentation

- [`CLAUDE.md`](CLAUDE.md) — Instructions and context for AI agents working on this codebase
- [`ref/architecture.md`](ref/architecture.md) — Workspace structure, data flow, design decisions
- [`ref/core.md`](ref/core.md) — Core library: DB, tokenizer, indexer, search, watcher
- [`ref/cli.md`](ref/cli.md) — CLI commands, config, entry points
- [`ref/mcp.md`](ref/mcp.md) — MCP server protocol, tools, Claude Desktop setup
- [`ref/models.md`](ref/models.md) — Data models, FTS5 query format, file hash
- [`docs/INSTALL.md`](docs/INSTALL.md) — Installation guide
- [`docs/CLI-USE.md`](docs/CLI-USE.md) — Detailed CLI reference
- [`docs/MCP-SETUP.md`](docs/MCP-SETUP.md) — Multi-vault MCP setup guide
- [`docs/FTS5.md`](docs/FTS5.md) — FTS5 query syntax and tips
- [`docs/Support-PDF.md`](docs/Support-PDF.md) — PDF/VLM feature details (required reading before enabling VLM)
