.DEFAULT_GOAL := help

ELPH_BIN   := elph
CARGO      := $$(which cargo)
CROSS      := $$(which cross)
UNAME_S    := $(shell uname -s)

_ELPH_PKGS   := elph elph-core elph-agent elph-ai
ELPH_VERSION  := $(shell grep '^version' elph/Cargo.toml | head -1 | sed 's/.*= *"\(.*\)"/\1/')
BUILD_HASH    := $(shell git rev-parse --short HEAD 2>/dev/null || echo "dev")
APP_BINS      := $(ELPH_BIN)
INSTALL_DIR   := $(HOME)/.local/bin
APP           ?= elph

# ─── Compiler cache ───────────────────────────────────────────────────────────
# Use sccache when installed; otherwise leave RUSTC_WRAPPER unset (normal rustc).
SCCACHE_BIN := $(shell command -v sccache 2>/dev/null)
ifneq ($(SCCACHE_BIN),)
  export RUSTC_WRAPPER := sccache
  export SCCACHE_DIRECT := true
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

# Build profile: debug by default (faster for day-to-day install).
# Release (any of):
#   make install RELEASE=1
#   make install -- --release
#   make build -- --release
# Note: `make install --release` is rejected by GNU make (unknown option). Use `-- --release`.
# Do not use residual goal `release` — it collides with the cross `release` target.
_RELEASE_REQUESTED :=
ifneq ($(filter 1 true yes,$(RELEASE)),)
  _RELEASE_REQUESTED := 1
endif
ifneq ($(filter release,$(PROFILE)),)
  _RELEASE_REQUESTED := 1
endif
ifneq ($(filter --release,$(MAKECMDGOALS) $(_RESIDUAL_)),)
  _RELEASE_REQUESTED := 1
endif

ifeq ($(_RELEASE_REQUESTED),1)
  CARGO_BUILD_FLAGS := --release
  BUILD_PROFILE     := release
else
  CARGO_BUILD_FLAGS :=
  BUILD_PROFILE     := debug
endif
BUILD_DIR := ./target/$(BUILD_PROFILE)

.PHONY: build build-elph install run watch test test-elph test-elph-tui check-elph check-elph-tui generate-models prepare
.PHONY: lint lint-elph lint-elph-tui build-elph-tui-examples fmt clean check coverage help stats
.PHONY: cross cross-pull release release-linux release-macos release-windows
.PHONY: bump bump-elph bump-libs publish publish-dry-run version

# ─── Build ──────────────────────────────────────────────────────────────────

check: ## Check code compiles (fast, no codegen)
	@$(CARGO) check --workspace 2>&1
# 	@$(CARGO) bloat --release -n 50

build: build-elph ## Build elph binary (debug default; RELEASE=1 or -- --release)

build-elph: ## Build elph binary (debug default; RELEASE=1 or -- --release)
	@echo "Building $(ELPH_BIN) v$(ELPH_VERSION) ($(BUILD_HASH)) [$(BUILD_PROFILE)] ($$RUSTC_WRAPPER)"
	@_start=$$(python3 -c "import time; print(int(time.time()*1000))"); \
	$(CARGO) build $(CARGO_BUILD_FLAGS) --bin $(ELPH_BIN) 2>&1; \
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

install: build ## Install elph-next (debug default; RELEASE=1 or -- --release)
	@mkdir -p $(INSTALL_DIR) && echo
	@for bin in $(APP_BINS); do \
	  cp "$(BUILD_DIR)/$$bin" "$(INSTALL_DIR)/$$bin-next"; \
	  echo "$$bin-next installed at: $(INSTALL_DIR)/$$bin-next [$(BUILD_PROFILE)]"; \
	done

run: ## Run elph coding agent
	@_args='$(or $(_RESIDUAL_),$(ARGS))'; \
	if [ -n "$$_args" ]; then \
		$(CARGO) run -q -p $(ELPH_BIN) -- $$_args; \
	else \
		$(CARGO) run -q -p $(ELPH_BIN); \
	fi

watch: ## Run elph with hot reload (requires watchexec)
	@-$(CARGO) watch -c -- cargo run --bin $(ELPH_BIN) $(or $(_RESIDUAL_),$(ARGS)) 2>&1

test: ## Run all workspace tests
	@$(CARGO) nextest run --no-fail-fast $(or $(_RESIDUAL_),$(ARGS))

test-elph: ## Run tests for elph and its workspace deps
	@$(CARGO) nextest run --no-fail-fast -p elph-ai -p elph-core -p elph $(ARGS)
	@$(CARGO) nextest run --no-fail-fast -p elph-agent --features full $(ARGS)

test-elph-tui: ## Run elph-tui tests
	@$(CARGO) nextest run --no-fail-fast -p elph-tui $(ARGS)

check-elph: ## Check elph and its workspace deps compile
	@$(CARGO) check -p elph-ai -p elph-core -p elph 2>&1
	@$(CARGO) check -p elph-agent --features full --all-targets 2>&1

check-elph-tui: ## Check elph-tui compiles (lib, tests, examples)
	@$(CARGO) check -p elph-tui --all-targets 2>&1

build-elph-tui-examples: ## Build all elph-tui examples
	@$(CARGO) build -p elph-tui --examples 2>&1

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

cross: ## Build one platform (CROSS_TARGET=<triple>; APP=elph; CROSS_QUIET=1 / CROSS_VERBOSE=1)
	@test -n "$(CROSS_TARGET)" || { echo "Usage: make cross CROSS_TARGET=<triple>" >&2; exit 1; }
	@APP="$(APP)" ./scripts/cross-build.sh $(CROSS_TARGET) $(APP)

release: ## Build release (host-aware: cargo native, cross remote)
	@./scripts/cross-release.sh

release-linux: ## Build Linux release (glibc + musl, x86_64 + arm64; APP=elph)
	@APP="$(APP)" ./scripts/cross-platform.sh linux

release-macos: ## Build macOS release (x86_64 + arm64; APP=elph)
	@APP="$(APP)" ./scripts/cross-platform.sh macos

release-windows: ## Build Windows release (x86_64 + arm64; APP=elph)
	@APP="$(APP)" ./scripts/cross-platform.sh windows

# ─── Code Quality ───────────────────────────────────────────────────────────

lint: lint-elph ## Run clippy linter

lint-elph: ## Run clippy for elph and its workspace deps
	@$(CARGO) clippy -p elph -p elph-core -p elph-agent -p elph-ai --all-targets -- -D warnings

lint-elph-tui: ## Run clippy for elph-tui
	@$(CARGO) clippy -p elph-tui --all-targets -- -D warnings

fmt: ## Format all code
	@$(CARGO) fmt --all -- --style-edition 2024
	@bunx --silent oxfmt crates/elph-ai/models/
	@bunx --silent oxfmt openwiki/ schemas/

coverage: ## Run tests with coverage (requires cargo-llvm-cov)
	@$(CARGO) llvm-cov nextest --no-cfg-coverage 2>&1

stats: ## Show sccache stats and code line count
	@tokei . -e "*.json" -e "*.md"
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
	@command -v cargo-llvm-cov >/dev/null 2>&1 || $(CARGO) binstall --locked -y cargo-llvm-cov
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
# version: compare Cargo.toml with latest GitHub releases
#   make version
#   make version APP=elph
#   make version TAG=elph-v0.0.28

version: ## Compare app versions with latest GitHub releases (APP=, TAG=)
	@APP="$(APP)" TAG="$(TAG)" ./scripts/version.sh

# Independent version streams:
#   bump-elph  — elph/Cargo.toml
#   bump-libs  — crates/elph-{core,agent,ai,tui,swarm} (+ workspace pins)
#   bump       — bump-libs + bump-elph
#
# Usage (level is required):
#   make bump       patch|minor|major
#   make bump-elph  patch|minor|major
#   make bump-libs  patch|minor|major

ifeq ($(UNAME_S),Darwin)
  SED_INPLACE := sed -i ''
else
  SED_INPLACE := sed -i
endif

_BUMP_LEVEL := $(firstword $(_RESIDUAL_))
_BUMP_PY    := python3 -c "import sys;m,M,p=sys.argv[1].split('.');l=sys.argv[2];print(f'{m}.{M}.{int(p)+1}' if l=='patch' else f'{m}.{int(M)+1}.0' if l=='minor' else f'{int(m)+1}.0.0')"

_LIBS := elph-core elph-ai elph-agent elph-swarm

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

bump: ## Bump all libs and elph (patch|minor|major required)
	$(call _require_bump_level,$(_BUMP_LEVEL),bump)
	@echo "bump ($(_BUMP_LEVEL))..."
	@$(MAKE) --no-print-directory bump-libs $(_BUMP_LEVEL)
	@$(MAKE) --no-print-directory bump-elph $(_BUMP_LEVEL)
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
	@printf ' \n\033[33mBuild profile (build / install):\033[0m\n'
	@printf ' \033[36mmake install\033[0m                  debug (default, faster)\n'
	@printf ' \033[36mmake install RELEASE=1\033[0m        release optimized\n'
	@printf ' \033[36mmake install -- --release\033[0m     release (GNU make end-of-options)\n'
	@printf ' \033[36mmake build PROFILE=release\033[0m    same as RELEASE=1\n'
	@printf ' note: \033[36mmake install --release\033[0m  is invalid (make option parse)\n'
