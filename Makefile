MODEL_FILE = models/bccwj-suw+unidic_pos+kana.model.zst
PREFIX    ?= /usr/local
BINDIR     = $(PREFIX)/bin

.DEFAULT_GOAL := build

# Download model if not present
$(MODEL_FILE):
	./scripts/download-model.sh

model: $(MODEL_FILE)

build: $(MODEL_FILE)
	SHIOTSUCHI_EMBED_MODEL=$(CURDIR)/$(MODEL_FILE) cargo build --release

build-dev:
	cargo build

test: $(MODEL_FILE)
	SHIOTSUCHI_MODEL_PATH=$(CURDIR)/$(MODEL_FILE) cargo test --workspace

bench: $(MODEL_FILE)
	SHIOTSUCHI_MODEL_PATH=$(CURDIR)/$(MODEL_FILE) \
	  cargo bench -p obsidian-shiotsuchi-vault-core

install: build
	install -d $(BINDIR)
	install -m 755 target/release/shiotsuchi $(BINDIR)/
	install -m 755 target/release/shiotsuchi-mcp $(BINDIR)/

uninstall:
	rm -f $(BINDIR)/shiotsuchi \
	      $(BINDIR)/shiotsuchi-mcp

integration-test: build
	cd integration && npm install --silent && npm test

test-all: clean test integration-test

clean:
	cargo clean
	rm -rf /tmp/shiotsuchi-test-vault

help:
	@echo "Usage: make [target] [PREFIX=/usr/local]"
	@echo ""
	@echo "Targets:"
	@echo "  build       Build release binaries (embeds tokenizer model)"
	@echo "  build-dev   Build dev profile (no model embedding)"
	@echo "  test             Run all Rust workspace tests"
	@echo "  integration-test Run Vitest MCP integration tests"
	@echo "  test-all         Run all tests (Rust + Vitest)"
	@echo "  bench            Run criterion benchmarks"
	@echo "  install          Install binaries to \$$(PREFIX)/bin  [default: /usr/local/bin]"
	@echo "  uninstall        Remove installed binaries"
	@echo "  model            Download tokenizer model"
	@echo "  clean            Remove build artifacts"
	@echo "  help             Show this help"

.PHONY: build build-dev test bench install uninstall clean help model integration-test test-all
