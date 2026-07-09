.DEFAULT_GOAL := help

ELPH_BIN     := elph
ECLAW_BIN    := eclaw
OWLY_BIN     := owly
CARGO        := $$(which cargo)
CROSS        := $$(which cross)
ELPH_VERSION  := $(shell grep '^version' elph/Cargo.toml | head -1 | sed 's/.*= *"\(.*\)"/\1/')
ECLAW_VERSION := $(shell grep '^version' eclaw/Cargo.toml | head -1 | sed 's/.*= *"\(.*\)"/\1/')
OWLY_VERSION  := $(shell grep '^version' owly/Cargo.toml | head -1 | sed 's/.*= *"\(.*\)"/\1/')
BUILD_HASH   := $(shell git rev-parse --short HEAD 2>/dev/null || echo "dev")
APP_BINS     := $(ELPH_BIN) $(ECLAW_BIN) $(OWLY_BIN)
INSTALL_DIR  := $(HOME)/.local/bin
BUILD_DIR    := ./target/release

UNAME_S := $(shell uname -s)

# ─── Compiler cache ───────────────────────────────────────────────────────────
# Use sccache when installed; otherwise leave RUSTC_WRAPPER unset (normal rustc).
SCCACHE_BIN := $(shell command -v sccache 2>/dev/null)
ifneq ($(SCCACHE_BIN),)
  export RUSTC_WRAPPER := sccache
endif

# Single-platform override: make cross CROSS_TARGET=aarch64-unknown-linux-musl

# ─── Args ───────────────────────────────────────────────────────────────────

# Named args:  make run ARGS="-- --nocapture"  /  make test PKG=foo
# catalog:    make generate-models ELPH_AI_CATALOG_DIR=/path/to/catalog/packages/ai ARGS="--skip-scripts"
ELPH_AI_CATALOG_DIR  ?= ../catalog/packages/ai
ARGS       :=
_RESIDUAL_ := $(wordlist 2,$(words $(MAKECMDGOALS)),$(MAKECMDGOALS))
$(foreach a,$(_RESIDUAL_),$(eval .PHONY: $a))
$(foreach a,$(_RESIDUAL_),$(eval $a: ; @true))

.PHONY: build run watch test lint fmt clean check coverage help stats generate-models
.PHONY: prepare cross cross-pull release build-linux build-macos build-windows
.PHONY: bump bump-elph bump-eclaw bump-owly bump-libs publish publish-dry-run

# ─── Build ──────────────────────────────────────────────────────────────────

check: ## Check code compiles (fast, no codegen)
	@$(CARGO) check --workspace 2>&1
	@$(CARGO) bloat --release -n 50

build: ## Build all application binaries (elph + eclaw + owly)
	@echo "Building elph v$(ELPH_VERSION), eclaw v$(ECLAW_VERSION), owly v$(OWLY_VERSION) ($(BUILD_HASH)) ($$RUSTC_WRAPPER)"
	@_start=$$(python3 -c "import time; print(int(time.time()*1000))"); \
	$(CARGO) build --release 2>&1; \
	_end=$$(python3 -c "import time; print(int(time.time()*1000))"); \
	_elapsed=$$(( _end - _start )); \
	echo ""; \
	for bin in $(APP_BINS); do \
	  if [ -f "$(BUILD_DIR)/$$bin" ]; then \
	    if command -v rapidhash >/dev/null 2>&1; then \
	      hash=$$(rapidhash "$(BUILD_DIR)/$$bin"); \
	    elif command -v sha256sum >/dev/null 2>&1; then \
	      hash=$$(sha256sum "$(BUILD_DIR)/$$bin" | cut -d' ' -f1); \
	    else \
	      hash=$$(shasum -a 256 "$(BUILD_DIR)/$$bin" | cut -d' ' -f1); \
	    fi; \
	    echo "Binary $$bin:$$(du -sh $(BUILD_DIR)/$$bin | cut -f1) ($$hash)"; \
	  else \
	    echo "Binary $$bin:(not built)"; \
	  fi; \
	done; \
	printf "Build time: %d.%03ds\n" $$(( _elapsed / 1000 )) $$(( _elapsed % 1000 ))

install: build ## Install elph-next, eclaw-next, and owly to $INSTALL_DIR
	@mkdir -p $(INSTALL_DIR) && echo
	@for bin in $(APP_BINS); do \
	  cp "$(BUILD_DIR)/$$bin" "$(INSTALL_DIR)/$$bin-next"; \
	  echo "$$bin-next installed at: $(INSTALL_DIR)/$$bin-next"; \
	done

run: ## Run elph coding agent
	@$(CARGO) run --bin $(ELPH_BIN) $(or $(_RESIDUAL_),$(ARGS))

run-owly: ## Run owly documentation agent
	@$(CARGO) run --bin $(OWLY_BIN) $(or $(_RESIDUAL_),$(ARGS))
watch: ## Run eclaw with hot reload (requires watchexec)
	@-$(CARGO) watch -c -- cargo run --bin $(ECLAW_BIN) $(or $(_RESIDUAL_),$(ARGS)) 2>&1

test: ## Run all workspace tests
	@$(CARGO) nextest run --no-fail-fast $(or $(_RESIDUAL_),$(ARGS))

generate-models: ## Regenerate elph-ai model catalogs from catalog source (ELPH_AI_CATALOG_DIR, ARGS=--skip-scripts)
	@test -f "$(ELPH_AI_CATALOG_DIR)/scripts/generate-models.ts" || { \
	  echo "catalog source not found at: $(ELPH_AI_CATALOG_DIR)" >&2; \
	  echo "Set ELPH_AI_CATALOG_DIR to the packages/ai root, e.g. ELPH_AI_CATALOG_DIR=/path/to/catalog/packages/ai" >&2; \
	  exit 1; \
	}
	@$(CARGO) run -p elph-ai --bin generate-models -- all --catalog-dir "$(ELPH_AI_CATALOG_DIR)" $(ARGS)

# ─── Cross-Compilation ─────────────────────────────────────────────────────────
# Output: release/archives/ and release/binaries/ (+ SHA256SUMS each)
#   Linux: linux-glibc-* and linux-musl-* (not alpine-*)
#   linux-glibc-*  Ubuntu / Raspberry Pi OS 64-bit (glibc, Pi 3/4/5)
#   linux-musl-*   Alpine Linux (musl)
#   macos-*        macOS (native build on Mac)
#   win-*          Windows

cross-pull: ## Pull ghcr.io/cross-rs images into local Docker cache
	@./scripts/cross-pull-images.sh

cross: ## Build one platform (CROSS_TARGET=<triple>; CROSS_QUIET=1 / CROSS_VERBOSE=1)
	@test -n "$(CROSS_TARGET)" || { echo "Usage: make cross CROSS_TARGET=<triple>" >&2; exit 1; }
	@./scripts/cross-build.sh $(CROSS_TARGET)

release: ## Build release (host-aware: cargo native, cross remote)
	@./scripts/cross-release.sh

build-linux: ## Build Linux release (glibc + musl, x86_64 + arm64)
	@./scripts/cross-platform.sh linux

build-macos: ## Build macOS release (x86_64 + arm64)
	@./scripts/cross-platform.sh macos

build-windows: ## Build Windows release (x86_64 + arm64)
	@./scripts/cross-platform.sh windows

# ─── Code Quality ───────────────────────────────────────────────────────────

lint: ## Run clippy linter
	@$(CARGO) clippy --workspace --all-targets -- -D warnings

fmt: ## Format all code
	@$(CARGO) fmt --all

coverage: ## Run tests with coverage (requires tarpaulin)
	@$(CARGO) tarpaulin --workspace 2>&1

stats: ## Show sccache stats and code line count
	@tokei .
	@if [ -n "$(SCCACHE_BIN)" ]; then \
	  echo ""; \
	  printf '\033[33msccache stats:\033[0m\n'; \
	  "$(SCCACHE_BIN)" --show-stats; \
	fi

clean: ## Clean build artifacts and caches
	@find crates -type f -name '*_gen.rs' -delete
	@$(CARGO) clean

# ─── Misc ───────────────────────────────────────────────────────────────────

prepare: ## Install required toolchain
	@command -v cargo-binstall >/dev/null 2>&1 || $(CARGO) install cargo-binstall --locked
	@command -v cargo-bloat >/dev/null 2>&1 || $(CARGO) binstall --locked -y cargo-bloat
	@command -v cargo-nextest >/dev/null 2>&1 || $(CARGO) binstall --locked -y cargo-nextest
	@command -v cargo-tarpaulin >/dev/null 2>&1 || $(CARGO) binstall --locked -y cargo-tarpaulin
	@command -v watchexec >/dev/null 2>&1 || $(CARGO) binstall --locked -y watchexec-cli
	@command -v rapidhash >/dev/null 2>&1 || $(CARGO) install --locked -y rapidhash
	@command -v sccache >/dev/null 2>&1 || $(CARGO) binstall --locked -y sccache
	@command -v tokei >/dev/null 2>&1 || $(CARGO) binstall --locked -y tokei
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
# Independent version streams:
#   bump-elph  — elph/Cargo.toml
#   bump-eclaw — eclaw/Cargo.toml
#   bump-owly  — owly/Cargo.toml
#   bump-libs  — crates/elph-{core,agent,ai,tui,swarm} (+ workspace pins)
#   bump       — bump-libs + bump-elph + bump-eclaw + bump-owly
#
# Usage (level is required):
#   make bump       patch|minor|major
#   make bump-elph  patch|minor|major
#   make bump-eclaw patch|minor|major
#   make bump-owly  patch|minor|major
#   make bump-libs  patch|minor|major


ifeq ($(UNAME_S),Darwin)
  SED_INPLACE := sed -i ''
else
  SED_INPLACE := sed -i
endif

_BUMP_LEVEL := $(firstword $(_RESIDUAL_))
_BUMP_PY    := python3 -c "import sys;m,M,p=sys.argv[1].split('.');l=sys.argv[2];print(f'{m}.{M}.{int(p)+1}' if l=='patch' else f'{m}.{int(M)+1}.0' if l=='minor' else f'{int(m)+1}.0.0')"

_LIBS := elph-core elph-ai elph-agent elph-swarm elph-tui

define _require_bump_level
	@case "$(1)" in patch|minor|major) ;; *) \
	  echo "Usage: make $(2) {patch|minor|major}" >&2; \
	  exit 1;; esac
endef

define _bump_manifest
	@_f="$(1)"; _l="$(2)"; \
	_cur=$$(grep '^version = ' "$$_f" | head -1 | sed 's/.*= *"\(.*\)"/\1/'); \
	_new=$$($(_BUMP_PY) "$$_cur" "$$_l"); \
	$(SED_INPLACE) "s/^version = \"[^\"]*\"/version = \"$$_new\"/" "$$_f"; \
	echo "  $$_f: $$_cur → $$_new"
endef

define _sync_workspace_pin
	@_crate="$(1)"; \
	_ver=$$(grep '^version = ' "crates/$$_crate/Cargo.toml" | head -1 | sed 's/.*= *"\(.*\)"/\1/'); \
	$(SED_INPLACE) "s/\($$_crate = { path = \"crates\/$$_crate\", version = \)\"[^\"]*\"/\1\"$$_ver\"/" Cargo.toml; \
	echo "  Cargo.toml: $$_crate → $$_ver"
endef

bump-elph: ## Bump elph app version (patch|minor|major required)
	$(call _require_bump_level,$(_BUMP_LEVEL),bump-elph)
	@echo "bump-elph ($(_BUMP_LEVEL))..."
	$(call _bump_manifest,elph/Cargo.toml,$(_BUMP_LEVEL))
	@echo "Done."

bump-eclaw: ## Bump eclaw app version (patch|minor|major required)
	$(call _require_bump_level,$(_BUMP_LEVEL),bump-eclaw)
	@echo "bump-eclaw ($(_BUMP_LEVEL))..."
	$(call _bump_manifest,eclaw/Cargo.toml,$(_BUMP_LEVEL))
	@echo "Done."

bump-owly: ## Bump owly app version (patch|minor|major required)
	$(call _require_bump_level,$(_BUMP_LEVEL),bump-owly)
	@echo "bump-owly ($(_BUMP_LEVEL))..."
	$(call _bump_manifest,owly/Cargo.toml,$(_BUMP_LEVEL))
	@echo "Done."

bump-libs: ## Bump all library crates independently (patch|minor|major required)
	$(call _require_bump_level,$(_BUMP_LEVEL),bump-libs)
	@echo "bump-libs ($(_BUMP_LEVEL))..."
	@for c in $(_LIBS); do \
	  $(MAKE) --no-print-directory _bump_lib LIB=$$c LEVEL=$(_BUMP_LEVEL); \
	done
	@for c in $(_LIBS); do \
	  $(MAKE) --no-print-directory _sync_lib_pin LIB=$$c; \
	done
	@echo "Done."

bump: ## Bump all libs and apps independently (patch|minor|major required)
	$(call _require_bump_level,$(_BUMP_LEVEL),bump)
	@echo "bump ($(_BUMP_LEVEL))..."
	@$(MAKE) --no-print-directory bump-libs $(_BUMP_LEVEL)
	@$(MAKE) --no-print-directory bump-elph $(_BUMP_LEVEL)
	@$(MAKE) --no-print-directory bump-eclaw $(_BUMP_LEVEL)
	@$(MAKE) --no-print-directory bump-owly $(_BUMP_LEVEL)
	@echo "Done."

_bump_lib:
	$(call _bump_manifest,crates/$(LIB)/Cargo.toml,$(LEVEL))

_sync_lib_pin:
	$(call _sync_workspace_pin,$(LIB))

.PHONY: _bump_lib _sync_lib_pin

publish: ## Publish to crates.io (elph-core first, then libs, then apps)
	@CARGO="$(CARGO)" ./scripts/publish-crates.sh

publish-dry-run: ## Dry-run publish checks (elph-core first)
	@DRY_RUN=1 CARGO="$(CARGO)" ./scripts/publish-crates.sh

# ─── Help ───────────────────────────────────────────────────────────────────

help: ## Show this help
	@printf '\033[33mUsage:\033[0m make \033[36m<target>\033[0m\n'
	@awk -F ':.*## ' '/^[a-zA-Z_-]+:.*## / {printf " \033[36m%-18s\033[0m %s\n", $$1, $$2}' $(MAKEFILE_LIST)
