.DEFAULT_GOAL := help

CARGO       := cargo
BINARY_NAME := elph
BUILD_DIR   := ./target/release
PKG_VERSION := $(shell grep '^version' crates/coding-agent/Cargo.toml | head -1 | sed 's/.*= *"\(.*\)"/\1/')
BUILD_HASH  := $(shell git rev-parse --short HEAD 2>/dev/null || echo "dev")

# ─── Args ───────────────────────────────────────────────────────────────────

# Named args:  make run ARGS="-- --nocapture"  /  make test PKG=foo
ARGS       :=
_RESIDUAL_ := $(wordlist 2,$(words $(MAKECMDGOALS)),$(MAKECMDGOALS))
$(foreach a,$(_RESIDUAL_),$(eval .PHONY: $a))
$(foreach a,$(_RESIDUAL_),$(eval $a: ; @true))

.PHONY: build run watch test lint fmt clean check coverage prepare help

# ─── Build ──────────────────────────────────────────────────────────────────

check: ## Check code compiles (fast, no codegen)
	@$(CARGO) check --workspace 2>&1

build: ## Build the application binary
	@echo "Building Elph v$(PKG_VERSION) ($(BUILD_HASH))"
	@_start=$$(python3 -c "import time; print(int(time.time()*1000))"); \
	$(CARGO) build --release 2>&1; \
	_end=$$(python3 -c "import time; print(int(time.time()*1000))"); \
	_elapsed=$$(( _end - _start )); \
	echo "Binary size: $$(du -sh $(BUILD_DIR)/$(BINARY_NAME) | cut -f1) ($$(shasum -a 1 $(BUILD_DIR)/$(BINARY_NAME) | cut -d' ' -f1))"; \
	echo "Binary file: $(BUILD_DIR)/$(BINARY_NAME)"; \
	printf "Build time:  %d.%03ds\n" $$(( _elapsed / 1000 )) $$(( _elapsed % 1000 ))

run: ## Run the application
	@$(CARGO) run --bin $(BINARY_NAME) $(or $(_RESIDUAL_),$(ARGS))

watch: ## Run with hot reload (requires watchexec)
	@-$(CARGO) watch -c -- cargo run --bin $(BINARY_NAME) 2>&1

test: ## Run all workspace tests
	@$(CARGO) test --workspace $(or $(_RESIDUAL_),$(ARGS))

# ─── Code Quality ───────────────────────────────────────────────────────────

lint: ## Run clippy linter
	@$(CARGO) clippy --workspace -- -D warnings

fmt: ## Format all code
	@$(CARGO) fmt --all

coverage: ## Run tests with coverage (requires cargo-tarpaulin)
	@$(CARGO) tarpaulin --workspace 2>&1

clean: ## Clean build artifacts
	@$(CARGO) clean

# ─── Misc ───────────────────────────────────────────────────────────────────

prepare: ## Install required toolchain
	@command -v cargo-binstall >/dev/null 2>&1 || $(CARGO) install cargo-binstall --locked
	@command -v cargo-tarpaulin >/dev/null 2>&1 || $(CARGO) binstall --locked -y cargo-tarpaulin
	@command -v watchexec >/dev/null 2>&1 || $(CARGO) binstall --locked -y watchexec-cli

# ─── Help ───────────────────────────────────────────────────────────────────

help: ## Show this help
	@printf '\033[33mUsage:\033[0m make \033[36m<target>\033[0m\n'
	@awk -F ':.*## ' '/^[a-zA-Z_-]+:.*## / {printf " \033[36m%-18s\033[0m %s\n", $$1, $$2}' $(MAKEFILE_LIST)
