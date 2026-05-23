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

## Option A — Install via cargo

The fastest way to install — no git clone required.

### From crates.io

```sh
cargo install shiotsuchi shiotsuchi-mcp
```

### From git (latest main)

```sh
cargo install --git https://github.com/armaniacs/shiotsuchi-search shiotsuchi shiotsuchi-mcp
```

> **Model required at runtime.** `cargo install` does not embed the Vaporetto tokenizer model into
> the binary. Before running `shiotsuchi`, download the model and point `SHIOTSUCHI_MODEL_PATH`
> at it:
>
> ```sh
> # Download the model (requires curl)
> curl -sL "https://github.com/daac-tools/vaporetto-models/releases/download/v0.5.0/bccwj-suw+unidic_pos+kana.tar.xz" \
>   | tar -xJf - --strip-components=1 "bccwj-suw+unidic_pos+kana/bccwj-suw+unidic_pos+kana.model.zst"
> mkdir -p ~/.local/share/shiotsuchi
> mv bccwj-suw+unidic_pos+kana.model.zst ~/.local/share/shiotsuchi/
>
> # Add to ~/.bashrc or ~/.zshrc
> export SHIOTSUCHI_MODEL_PATH="$HOME/.local/share/shiotsuchi/bccwj-suw+unidic_pos+kana.model.zst"
> ```
>
> Once set, continue with the **Verify** and **First use** steps below.

## Option B — Build from source (model embedded, recommended)

### 1. Clone the repository

```sh
git clone https://github.com/armaniacs/shiotsuchi-search.git
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
[database]
db_path = "/Users/yourname/.cache/shiotsuchi/db.sqlite3"

[vaults.default]
notes_dir = "/Users/yourname/Notes"
```

Other available settings:

```toml
[database]
db_path = "/Users/yourname/.cache/shiotsuchi/db.sqlite3"

[vaults.default]
notes_dir  = "/Users/yourname/Notes"

[indexing]
snippet_lines       = 3
max_snippet_chars   = 1000
include_extensions  = ["md", "markdown"]
exclude_dirs         = ["node_modules"]

[watcher]
enabled = true
```

### Watch for changes

Keep the index updated automatically as you edit notes:

```sh
shiotsuchi scan --notes-dir ~/Notes
```

## Tokenizer model

The Vaporetto model (`bccwj-suw+unidic_pos+kana`) is compiled into the binary at build time — no separate model file is needed at runtime. The `make model` target downloads it independently if you only want to fetch it without building.

## Vector search (semantic search) model

To use `dive --mode vec` or `--mode hybrid`, you need to place an ONNX embedding model on disk separately.

### Supported models

| Model | Dimensions | Notes |
|-------|-----------|-------|
| [Qwen/Qwen3-Embedding-0.6B](https://huggingface.co/Qwen/Qwen3-Embedding-0.6B) | 1024 | Recommended — multilingual, lightweight |

### Download and placement

**Prerequisites for ONNX model**

The `hf` CLI tool (from `huggingface-hub`) is required for downloading the model. Install it first:

```sh
pip install huggingface-hub "optimum[onnxruntime]" sentence-transformers
```

After installation, log in to HuggingFace (optional but recommended for gated models):

```sh
hf auth login
```

**Option A — Manual download and conversion (recommended)**

The Qwen3-Embedding-0.6B model on HuggingFace provides `model.safetensors` and `tokenizer.json`, but not a pre-built ONNX file. You'll need to convert it:

```sh
hf download Qwen/Qwen3-Embedding-0.6B model.safetensors --local-dir /tmp/qwen3-embed
hf download Qwen/Qwen3-Embedding-0.6B tokenizer.json --local-dir /tmp/qwen3-embed

# Convert to ONNX using optimum-cli
optimum-cli export onnx -m Qwen/Qwen3-Embedding-0.6B /tmp/qwen3-onnx --task sentence-similarity --library-name sentence_transformers

# OR using sentence-transformers:
pip install sentence-transformers
python -c "
from sentence_transformers import SentenceTransformer
model = SentenceTransformer('Qwen/Qwen3-Embedding-0.6B')
model.save('/tmp/qwen3-onnx')
"

mkdir -p ~/.local/share/shiotsuchi
cp /tmp/qwen3-onnx/model.onnx ~/.local/share/shiotsuchi/model.onnx
cp /tmp/qwen3-onnx/model.onnx_data ~/.local/share/shiotsuchi/ 2>/dev/null || true
cp /tmp/qwen3/tokenizer.json ~/.local/share/shiotsuchi/
```

**Option B — `make onnx`**

```sh
make onnx   # Downloads model files and prints conversion instructions if no pre-built ONNX exists
```

**Option C — `make prepare`**

Download both models at once (ONNX requires `huggingface-hub`):

```sh
make prepare  # Downloads tokenizer + ONNX files
```

**Note:** The ONNX embedding model must be converted from safetensors. The `make onnx` script will attempt to find a pre-built ONNX file in the HuggingFace repository; if none exists, it downloads `model.safetensors` and prints conversion instructions.

### Model path resolution order

The first path that resolves to an existing file is used:

1. `--model-path /path/to/model.onnx` (CLI flag — highest priority)
2. `SHIOTSUCHI_EMBED_MODEL_PATH` environment variable
3. `~/.local/share/shiotsuchi/model.onnx` (XDG default)

### Verify

```sh
shiotsuchi setup --check
shiotsuchi dive --mode hybrid "your query"
```

If no model is found, `dive` falls back to FTS (keyword search) automatically. Passing `--mode vec` explicitly returns an error when no model is available.

## Troubleshooting

| Symptom | Fix |
|---------|-----|
| `no model available` after `cargo install` | Download model and set `SHIOTSUCHI_MODEL_PATH` — see Option A above |
| `command not found: shiotsuchi` | Add `~/.local/bin` (or `~/.cargo/bin`) to `PATH` |
| `rustc: command not found` | Install Rust via `curl https://sh.rustup.rs -sSf \| sh` |
| `curl: command not found` | Install curl via your package manager |
| Model download fails | Check your network, or download `models/bccwj-suw+unidic_pos+kana.model.zst` manually and re-run `make build` |
| Slow first build | Normal — Rust compiles all dependencies on the first run; subsequent builds are incremental |

## Further reading

- [README.md](../README.md) — Project overview, features, and commands
- [ref/cli.md](ref/cli.md) — All commands and options
- [ref/architecture.md](ref/architecture.md) — Design and data model
- [ref/mcp.md](ref/mcp.md) — MCP server setup for Claude Desktop
- [docs/MODEL_LICENSES.md](docs/MODEL_LICENSES.md) — License information for the bundled tokenizer model
