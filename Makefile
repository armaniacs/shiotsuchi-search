MODEL_FILE = models/bccwj-suw+unidic_pos+kana.model.zst
PREFIX    ?= /usr/local
BINDIR     = $(PREFIX)/bin

.DEFAULT_GOAL := build

# Download model if not present
$(MODEL_FILE):
	./scripts/download-model.sh

model: $(MODEL_FILE)

onnx:
	./scripts/download-onnx-model.sh

prepare:
	./scripts/download-model.sh
	@if command -v huggingface-cli >/dev/null 2>&1; then \
		./scripts/download-onnx-model.sh; \
	else \
		echo "Skipping ONNX model: huggingface-cli not found."; \
		echo "Install with: pip install huggingface-hub"; \
		echo "Then run: make onnx"; \
	fi

build: $(MODEL_FILE)
	SHIOTSUCHI_EMBED_MODEL=$(CURDIR)/$(MODEL_FILE) cargo build --release

build-dev:
	cargo build

test: $(MODEL_FILE)
	SHIOTSUCHI_MODEL_PATH=$(CURDIR)/$(MODEL_FILE) $(CURDIR)/scripts/test-timing.sh

test-fast:
	$(CURDIR)/scripts/test-timing.sh --fast

test-slow:
	SHIOTSUCHI_MODEL_PATH=$(CURDIR)/$(MODEL_FILE) \
	  $(CURDIR)/scripts/test-timing.sh --slow

test-retry-slow:
	$(CURDIR)/scripts/test-timing.sh --retry-slow

test-e2e: $(MODEL_FILE)
	cargo build -p shiotsuchi -p shiotsuchi-mcp
	SHIOTSUCHI_MODEL_PATH=$(CURDIR)/$(MODEL_FILE) cargo test -p shiotsuchi-e2e -- --nocapture

bench: $(MODEL_FILE)
	SHIOTSUCHI_MODEL_PATH=$(CURDIR)/$(MODEL_FILE) \
	  cargo bench -p shiotsuchi-core

install: build
	@INSTALL_DIR="$(BINDIR)"; \
	if [ "$(PREFIX)" = "/usr/local" ] && [ "$$(id -u)" -ne 0 ]; then \
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
	if [ "$(PREFIX)" = "/usr/local" ] && [ "$$(id -u)" -ne 0 ]; then \
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

# test-all requires Docker/Act installed for the local-ci target
test-all: test test-e2e integration-test local-ci

clean-all: clean

# On Apple Silicon (arm64), run arm64 containers natively to avoid QEMU linker crashes.
local-ci: $(MODEL_FILE)
	act $$( [ "$$(uname -m)" = "arm64" ] && echo "--container-architecture linux/arm64" )

.PHONY: doc
doc:
	@echo "Generating local documentation..."
	cargo doc --open --no-deps --document-private-items

.PHONY: doc-full
doc-full:
	@echo "Generating full documentation (including dependencies)..."
	cargo doc --open

# ドキュメントを完全に作り直す
doc-clean:
	rm -rf target/doc
	cargo doc --open --no-deps

clean:
	cargo clean
	rm -rf /tmp/shiotsuchi-test-vault

publish: $(MODEL_FILE)
	@echo "=== Running tests before publish ==="
	SHIOTSUCHI_MODEL_PATH=$(CURDIR)/$(MODEL_FILE) cargo test -p shiotsuchi-core -p shiotsuchi-mcp -p shiotsuchi
	@echo ""
	@echo "=== All tests passed. Publishing to crates.io ==="
	@echo ""
	@echo "--- Step 1/3: shiotsuchi-core ---"
	cd core && cargo publish
	@echo "--- Step 2/3: shiotsuchi (CLI) ---"
	cd cli && cargo publish
	@echo "--- Step 3/3: shiotsuchi-mcp ---"
	cd mcp && cargo publish
	@echo ""
	@echo "=== All crates published successfully ==="
	@echo "  cargo install shiotsuchi"
	@echo "  cargo install shiotsuchi-mcp"
	@echo "  cargo install --git https://github.com/armaniacs/shiotsuchi-search shiotsuchi"

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
	@echo "  local-ci         Run GitHub Actions CI locally using act (auto-detects architecture)"
	@echo "  bench            Run criterion benchmarks"
	@echo "  publish          Run tests, then publish all crates to crates.io in dependency order"
	@echo "  install          Install binaries to ~/.local/bin (or ~/.cargo/bin if exists) when not root, otherwise to $(PREFIX)/bin [default: /usr/local/bin]"
	@echo "  uninstall        Remove installed binaries"
	@echo "  model            Download tokenizer model"
	@echo "  onnx             Download ONNX embedding model (requires hf/huggingface-cli)"
	@echo "  prepare          Download tokenizer model + ONNX if hf installed"
	@echo "  clean            Remove build artifacts"
	@echo "  help             Show this help"

.PHONY: build build-dev test test-e2e bench publish install uninstall clean help model onnx prepare integration-test test-all local-ci doc doc-full doc-clean
