.DEFAULT_GOAL := help

ELPH_BIN     := elph
ECLAW_BIN    := eclaw
CARGO        := $$(which cargo)
CROSS        := $$(which cross)
PKG_VERSION  := $(shell grep '^version' elph/Cargo.toml | head -1 | sed 's/.*= *"\(.*\)"/\1/')
BUILD_HASH   := $(shell git rev-parse --short HEAD 2>/dev/null || echo "dev")
APP_BINS     := $(ELPH_BIN) $(ECLAW_BIN)
INSTALL_DIR  := $(HOME)/.local/bin
BUILD_DIR    := ./target/release

UNAME_S := $(shell uname -s)

# Single-platform override: make cross CROSS_TARGET=aarch64-unknown-linux-musl

# ─── Args ───────────────────────────────────────────────────────────────────

# Named args:  make run ARGS="-- --nocapture"  /  make test PKG=foo
ARGS       :=
_RESIDUAL_ := $(wordlist 2,$(words $(MAKECMDGOALS)),$(MAKECMDGOALS))
$(foreach a,$(_RESIDUAL_),$(eval .PHONY: $a))
$(foreach a,$(_RESIDUAL_),$(eval $a: ; @true))

.PHONY: build run watch test lint fmt clean check coverage help
.PHONY: prepare cross cross-pull release
.PHONY: build-linux-glibc build-linux-musl build-macos build-windows
.PHONY: bump-major bump-minor bump-patch publish

# ─── Build ──────────────────────────────────────────────────────────────────

check: ## Check code compiles (fast, no codegen)
	@$(CARGO) check --workspace 2>&1

build: ## Build all application binaries (elph + eclaw)
	@echo "Building workspace v$(PKG_VERSION) ($(BUILD_HASH))"
	@_start=$$(python3 -c "import time; print(int(time.time()*1000))"); \
	$(CARGO) build --release 2>&1; \
	_end=$$(python3 -c "import time; print(int(time.time()*1000))"); \
	_elapsed=$$(( _end - _start )); \
	echo ""; \
	for bin in $(APP_BINS); do \
	  if [ -f "$(BUILD_DIR)/$$bin" ]; then \
	    echo "Binary $$bin:$$(du -sh $(BUILD_DIR)/$$bin | cut -f1) ($$(shasum -a 1 $(BUILD_DIR)/$$bin | cut -d' ' -f1))"; \
	  else \
	    echo "Binary $$bin:(not built)"; \
	  fi; \
	done; \
	printf "Build time: %d.%03ds\n" $$(( _elapsed / 1000 )) $$(( _elapsed % 1000 ))

install: build ## Install elph-next and eclaw-next to $INSTALL_DIR
	@mkdir -p $(INSTALL_DIR) && echo
	@for bin in $(APP_BINS); do \
	  cp "$(BUILD_DIR)/$$bin" "$(INSTALL_DIR)/$$bin-next"; \
	  echo "$$bin-next installed at: $(INSTALL_DIR)/$$bin-next"; \
	done

run: ## Run elph coding agent
	@$(CARGO) run --bin $(ELPH_BIN) $(or $(_RESIDUAL_),$(ARGS))

watch: ## Run eclaw with hot reload (requires watchexec)
	@-$(CARGO) watch -c -- cargo run --bin $(ECLAW_BIN) $(or $(_RESIDUAL_),$(ARGS)) 2>&1

test: ## Run all workspace tests
	@$(CARGO) test --workspace $(or $(_RESIDUAL_),$(ARGS))

# ─── Cross-Compilation ─────────────────────────────────────────────────────────
# Output: release/archives/ and release/binaries/ (+ SHA256SUMS each)
#   Linux: linux-glibc-* and linux-musl-* (not alpine-*)
#   linux-glibc-*  Ubuntu / Raspberry Pi OS 64-bit (glibc, Pi 3/4/5)
#   linux-musl-*   Alpine Linux (musl)
#   macos-*        macOS (native build on Mac)
#   win-*          Windows

cross-pull: ## Pull ghcr.io/cross-rs images into local Docker cache
	@./scripts/cross-pull-images.sh

cross: ## Build one platform (CROSS_TARGET=<triple>, host-aware)
	@test -n "$(CROSS_TARGET)" || { echo "Usage: make cross CROSS_TARGET=<triple>" >&2; exit 1; }
	@./scripts/cross-build.sh $(CROSS_TARGET)

release: ## Build release (host-aware: cargo native, cross remote)
	@./scripts/cross-release.sh

build-linux-glibc: ## Build Linux glibc release (x86_64 + arm64)
	@./scripts/cross-platform.sh linux-glibc

build-linux-musl: ## Build Linux musl/alpine release (x86_64 + arm64)
	@./scripts/cross-platform.sh linux-musl

build-macos: ## Build macOS release (x86_64 + arm64)
	@./scripts/cross-platform.sh macos

build-windows: ## Build Windows release (x86_64 + arm64)
	@./scripts/cross-platform.sh windows

# ─── Code Quality ───────────────────────────────────────────────────────────

lint: ## Run clippy linter
	@$(CARGO) clippy --workspace -- -D warnings

fmt: ## Format all code
	@$(CARGO) fmt --all

coverage: ## Run tests with coverage (requires tarpaulin)
	@$(CARGO) tarpaulin --workspace 2>&1

clean: ## Clean build artifacts and caches
	@find crates -type f -name '*_gen.rs' -delete
	@$(CARGO) clean

# ─── Misc ───────────────────────────────────────────────────────────────────

prepare: ## Install required toolchain
	@command -v cargo-binstall >/dev/null 2>&1 || $(CARGO) install cargo-binstall --locked
	@command -v cargo-tarpaulin >/dev/null 2>&1 || $(CARGO) binstall --locked -y cargo-tarpaulin
	@command -v watchexec >/dev/null 2>&1 || $(CARGO) binstall --locked -y watchexec-cli
	@command -v cross >/dev/null 2>&1 || $(CARGO) install cross --locked
	@while read -r t; do rustup target add "$$t" 2>/dev/null || true; done < ./scripts/cross-targets.sh
	@if [ "$(UNAME_S)" = "Darwin" ]; then \
	  if xcrun --find metal 2>/dev/null >/dev/null; then \
	    echo "Metal toolchain already installed at $$(xcrun --find metal)"; \
	  else \
	    xcodebuild -downloadComponent MetalToolchain 2>&1; \
	  fi; \
	fi

# ─── Versioning ────────────────────────────────────────────────────────────────

_CUR := $(shell grep '^version' elph/Cargo.toml | head -1 | sed 's/.*= *"\(.*\)"/\1/')
_MAJ := $(word 1,$(subst ., ,$(_CUR)))
_MIN := $(word 2,$(subst ., ,$(_CUR)))
_PAT := $(word 3,$(subst ., ,$(_CUR)))

define _bump
	@echo "Bumping $(_CUR) → $(1)..."
	@for f in crates/*/Cargo.toml elph/Cargo.toml eclaw/Cargo.toml; do \
	  sed -i '' 's/^version = "[0-9]*\.[0-9]*\.[0-9]*"/version = "$(1)"/' "$$f"; \
	done
	@sed -i '' 's/\(elph-agent = .* version = \)"\^*[0-9]*\.[0-9]*\.[0-9]*"/\1"^$(1)"/' elph/Cargo.toml
	@sed -i '' 's/\(elph-tui = .* version = \)"\^*[0-9]*\.[0-9]*\.[0-9]*"/\1"^$(1)"/' elph/Cargo.toml
	@sed -i '' 's/\(elph-ai = .* version = \)"\^*[0-9]*\.[0-9]*\.[0-9]*"/\1"^$(1)"/' crates/agent/Cargo.toml
	@sed -i '' 's/\(elph-agent = .* version = \)"\^*[0-9]*\.[0-9]*\.[0-9]*"/\1"^$(1)"/' eclaw/Cargo.toml
endef

bump-patch: ## Bump patch (0.0.x)
	$(call _bump,$(_MAJ).$(_MIN).$(shell expr $(_PAT) + 1))

bump-minor: ## Bump minor (0.x.0)
	$(call _bump,$(_MAJ).$(shell expr $(_MIN) + 1).0)

bump-major: ## Bump major (x.0.0)
	$(call _bump,$(shell expr $(_MAJ) + 1).0.0)

publish: ## Publish all crates to crates.io
	@echo "Publishing elph-ai v$(PKG_VERSION) to crates.io" && $(CARGO) publish --quiet -p elph-ai --allow-dirty 2>&1
	@echo "Publishing elph-agent v$(PKG_VERSION) to crates.io" && $(CARGO) publish --quiet -p elph-agent --allow-dirty 2>&1
	@echo "Publishing elph-tui v$(PKG_VERSION) to crates.io" && $(CARGO) publish --quiet -p elph-tui --allow-dirty 2>&1
	@echo "Publishing elph v$(PKG_VERSION) to crates.io" && $(CARGO) publish --quiet -p elph --allow-dirty 2>&1
	@echo "Publishing eclaw v$(PKG_VERSION) to crates.io" && $(CARGO) publish --quiet -p eclaw --allow-dirty 2>&1
	@echo "All crates published."

# ─── Help ───────────────────────────────────────────────────────────────────

help: ## Show this help
	@printf '\033[33mUsage:\033[0m make \033[36m<target>\033[0m\n'
	@awk -F ':.*## ' '/^[a-zA-Z_-]+:.*## / {printf " \033[36m%-18s\033[0m %s\n", $$1, $$2}' $(MAKEFILE_LIST)
