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
	SHIOTSUCHI_MODEL_PATH=$(CURDIR)/$(MODEL_FILE) cargo test --workspace --exclude shiotsuchi-e2e

test-e2e: $(MODEL_FILE)
	cargo build -p shiotsuchi -p shiotsuchi-mcp
	SHIOTSUCHI_MODEL_PATH=$(CURDIR)/$(MODEL_FILE) cargo test -p shiotsuchi-e2e -- --nocapture

bench: $(MODEL_FILE)
	SHIOTSUCHI_MODEL_PATH=$(CURDIR)/$(MODEL_FILE) \
	  cargo bench -p obsidian-shiotsuchi-vault-core

install: build
	@INSTALL_DIR="$(BINDIR)"; \
	if [ "$(origin PREFIX)" = "default" ] && [ "$$(id -u)" -ne 0 ]; then \
		if [ -d "$$HOME/.local/bin" ]; then \
			INSTALL_DIR="$$HOME/.local/bin"; \
		elif [ -d "$$HOME/.cargo/bin" ]; then \
			INSTALL_DIR="$$HOME/.cargo/bin"; \
		else \
			INSTALL_DIR="$$HOME/.local/bin"; \
			mkdir -p "$$INSTALL_DIR"; \
		fi; \
	fi; \
	install -d "$$INSTALL_DIR"; \
	install -m 755 target/release/shiotsuchi "$$INSTALL_DIR"/; \
	install -m 755 target/release/shiotsuchi-mcp "$$INSTALL_DIR"/;

uninstall:
	@INSTALL_DIR="$(BINDIR)"; \
	if [ "$(origin PREFIX)" = "default" ] && [ "$$(id -u)" -ne 0 ]; then \
		if [ -d "$$HOME/.local/bin" ]; then \
			INSTALL_DIR="$$HOME/.local/bin"; \
		elif [ -d "$$HOME/.cargo/bin" ]; then \
			INSTALL_DIR="$$HOME/.cargo/bin"; \
		else \
			INSTALL_DIR="$$HOME/.local/bin"; \
		fi; \
	fi; \
	rm -f "$$INSTALL_DIR"/shiotsuchi \
	      "$$INSTALL_DIR"/shiotsuchi-mcp

integration-test: build
	cd integration && npm install --silent && npm test

test-all: clean test test-e2e integration-test

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
	@echo "  test-e2e         Run end-to-end integration tests"
	@echo "  integration-test Run Vitest MCP integration tests"
	@echo "  test-all         Run all tests (Rust + E2E + Vitest)"
	@echo "  bench            Run criterion benchmarks"
	@echo "  install          Install binaries to ~/.local/bin (or ~/.cargo/bin if exists) when not root, otherwise to $(PREFIX)/bin [default: /usr/local/bin]"
	@echo "  uninstall        Remove installed binaries"
	@echo "  model            Download tokenizer model"
	@echo "  clean            Remove build artifacts"
	@echo "  help             Show this help"

.PHONY: build build-dev test test-e2e bench install uninstall clean help model integration-test test-all
