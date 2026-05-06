# Installing shiotsuchi-search

shiotsuchi-search is a high-performance Japanese-aware search engine for Markdown note vaults, powered by [Vaporetto](https://github.com/daac-tools/vaporetto) × SQLite FTS5.

## Prerequisites

- **Rust** 1.75 or later — install via [rustup.rs](https://rustup.rs)
- **curl** — for downloading the tokenizer model
- **make** — available on macOS and most Linux distributions

Verify your Rust installation:

```sh
rustc --version   # should be 1.75+
cargo --version
```

## Install

### 1. Clone the repository

```sh
git clone https://github.com/araki-yzrh/shiotsuchi-search.git
cd shiotsuchi-search
```

### 2. Build and install

```sh
make install
```

This single command:

1. Downloads the Vaporetto tokenizer model into `models/` (if not already present)
2. Compiles the model into the binary at build time (`SHIOTSUCHI_EMBED_MODEL`)
3. Installs both binaries to the first available location:
   - `~/.local/bin/` (preferred for normal users)
   - `~/.cargo/bin/` (if it exists)
   - `/usr/local/bin/` (when running as root, or with `sudo`)

After installation you should have two commands on your `PATH`:

| Binary | Purpose |
|--------|---------|
| `shiotsuchi` | CLI — index, search, watch |
| `shiotsuchi-mcp` | MCP server for Claude Desktop |

### 3. Verify

```sh
shiotsuchi --help
```

If the shell cannot find the binary, add `~/.local/bin` to your `PATH`:

```sh
# bash / zsh — add to ~/.bashrc or ~/.zshrc
export PATH="$HOME/.local/bin:$PATH"
```

## Custom install prefix

To install into `/usr/local` (requires `sudo`) or a different prefix:

```sh
sudo make install PREFIX=/usr/local
# or
make install PREFIX=/opt/shiotsuchi
```

## Uninstall

```sh
make uninstall
# or, if a custom prefix was used:
sudo make uninstall PREFIX=/usr/local
```

## First use

### Index your vault

```sh
shiotsuchi chart --notes-dir ~/Notes
```

Replace `~/Notes` with the path to your Markdown vault. This walks every `.md` file, tokenizes the content, and writes a SQLite index to `~/.cache/shiotsuchi/db.sqlite3`.

### Search

```sh
shiotsuchi dive "project plan"
```

Results include file paths, titles, and matching snippets.

### Optional: configuration file

Create `~/.config/shiotsuchi/config.toml` to avoid passing flags every time:

```toml
[vault]
notes_dir = "/Users/yourname/Notes"
```

Other available settings:

```toml
[vault]
notes_dir  = "/Users/yourname/Notes"
db_path    = "/Users/yourname/.cache/shiotsuchi/db.sqlite3"

[indexing]
snippet_lines       = 3
include_extensions  = ["md", "markdown"]
exclude_patterns    = [".obsidian", ".git", "node_modules"]

[watcher]
debounce_ms = 500
enabled     = true
```

### Watch for changes

Keep the index updated automatically as you edit notes:

```sh
shiotsuchi scan --notes-dir ~/Notes
```

## Tokenizer model

The Vaporetto model (`bccwj-suw+unidic_pos+kana`) is compiled into the binary at build time — no separate model file is needed at runtime. The `make model` target downloads it independently if you only want to fetch it without building.

## Troubleshooting

| Symptom | Fix |
|---------|-----|
| `command not found: shiotsuchi` | Add `~/.local/bin` (or `~/.cargo/bin`) to `PATH` |
| `rustc: command not found` | Install Rust via `curl https://sh.rustup.rs -sSf \| sh` |
| `curl: command not found` | Install curl via your package manager |
| Model download fails | Check your network, or download `models/bccwj-suw+unidic_pos+kana.model.zst` manually and re-run `make build` |
| Slow first build | Normal — Rust compiles all dependencies on the first run; subsequent builds are incremental |

## Further reading

- [ref/cli.md](ref/cli.md) — All commands and options
- [ref/architecture.md](ref/architecture.md) — Design and data model
- [ref/mcp.md](ref/mcp.md) — MCP server setup for Claude Desktop
- [docs/MODEL_LICENSES.md](docs/MODEL_LICENSES.md) — License information for the bundled tokenizer model
