# Quick Setup — Test Model Installation

> This is a quick-start guide for setting up shiotsuchi-search with a **test/development** model.
> For production use, see [INSTALL.md](./INSTALL.md).

## Steps

### 1. Download the Vaporetto tokenizer model

```bash
make model
```

This downloads `models/bccwj-suw+unidic_pos+kana.model.zst`.

### 2. Build

```bash
make build
```

The tokenizer model is embedded into the binary at compile time via `SHIOTSUCHI_EMBED_MODEL`.
Output: `target/release/shiotsuchi` and `target/release/shiotsuchi-mcp`.

### 3. Install (optional)

```bash
make install
```

Binaries are installed to `~/.local/bin/` (or `~/.cargo/bin/` if the former doesn't exist).

### 4. Smoke test

```bash
# Show help
shiotsuchi --help

# Create a temporary test vault
mkdir -p /tmp/shiotsuchi-test-vault
echo '# Test Note' > /tmp/shiotsuchi-test-vault/test.md
echo 'Hello world' >> /tmp/shiotsuchi-test-vault/test.md

# Index the vault
shiotsuchi chart --notes-dir /tmp/shiotsuchi-test-vault

# Search
shiotsuchi dive "test" --notes-dir /tmp/shiotsuchi-test-vault

# Clean up
rm -rf /tmp/shiotsuchi-test-vault
```

## Cleanup

```bash
make clean
```

## Troubleshooting

| Symptom | Fix |
|---------|-----|
| `make model` fails | Run `scripts/download-model.sh` directly to see the error |
| Build fails | Verify Rust 1.75+: `rustc --version` |
| Model not found | Check `ls models/` for the downloaded file |

## Related files

- [`Makefile`](../Makefile) — build and download targets
- [`models/`](../models/) — model storage directory (gitignored)