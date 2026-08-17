.DEFAULT_GOAL := help

APP_BIN  := elph
CARGO    := $$(which cargo)
CROSS    := $$(which cross)
UNAME_S  := $(shell uname -s)
UNAME_M  := $(shell uname -m)

# On Apple Silicon macOS, default to Metal GPU acceleration for local embeddings
# (codegraph + memory). The `metal` feature only compiles there; other platforms
# stay on the CPU backend. Override with `make build ELPH_METAL=`.
ifeq ($(UNAME_S),Darwin)
  ifeq ($(UNAME_M),arm64)
    ELPH_METAL_FEATURE ?= --features metal
  endif
endif
ELPH_METAL_FEATURE ?=

_ELPH_PKGS   := elph elph-agent elph-ai
ELPH_VERSION := $(shell grep '^version' crates/coding-agent/Cargo.toml | head -1 | sed 's/.*= *"\(.*\)"/\1/')
BUILD_HASH   := $(shell git rev-parse --short HEAD 2>/dev/null || echo "dev")
APP_BINS     := $(APP_BIN)
INSTALL_DIR  := $(HOME)/.local/bin
APP          ?= elph

# ─── Compiler cache ───────────────────────────────────────────────────────────
# Use sccache when installed AND its daemon is responsive; otherwise disable it
# for this build. A stale daemon socket/lock (e.g. after a crashed build) causes
# "Timed out waiting for server startup" — --start-server recovers when it can.
# Override: SCCACHE_DISABLE=1 make build  (skip sccache entirely).
ifneq ($(SCCACHE_DISABLE),1)
  SCCACHE_BIN := $(shell command -v sccache 2>/dev/null)
  ifneq ($(SCCACHE_BIN),)
    # Probe daemon health via --show-stats (no storage re-init). --start-server
    # fails if remote cache (S3/R2) credentials are unreachable, even when the
    # local daemon is already running fine.
    SCCACHE_OK := $(shell "$(SCCACHE_BIN)" --show-stats >/dev/null 2>&1 && echo 1 || echo 0)
  endif
endif
ifneq ($(SCCACHE_OK),)
  ifeq ($(SCCACHE_OK),1)
    export AWS_PROFILE := r2-sccache
    export RUSTC_WRAPPER := sccache
    export SCCACHE_DIRECT := true
    # Cap remote cache at 50 GB so it never eclipses local disk. The bucket is shared
    # across sessions — anything beyond this size is unlikely to be re-used soon.
    export SCCACHE_MAXSIZE := 50G
  endif
endif

# Single-platform override: make cross CROSS_TARGET=aarch64-unknown-linux-musl

# ─── Args ───────────────────────────────────────────────────────────────────

# Named args:  make run ARGS="-- --nocapture"  /  make test PKG=foo
# catalog:    make generate-models ARGS="--skip-scripts"
# Catalog source path is fixed in generate-models
# (../../earendil-works/pi/packages/ai from workspace root).
ARGS       :=
_RESIDUAL_ := $(wordlist 2,$(words $(MAKECMDGOALS)),$(MAKECMDGOALS))
$(foreach a,$(_RESIDUAL_),$(eval .PHONY: $a))
$(foreach a,$(_RESIDUAL_),$(eval $a: ; @true))

# _after <list> <needle> — returns the words following the first occurrence of
# <needle> (pure make, used to extract `--features <name>` residual values).
define _after
$(if $(filter $(2),$(firstword $(1))),$(wordlist 2,$(words $(1)),$(1)),$(if $(wordlist 2,$(words $(1)),$(1)),$(call _after,$(wordlist 2,$(words $(1)),$(1)),$(2))))
endef

# Build profile: debug by default (faster for day-to-day install).
# Release (any of):
#   make install RELEASE=1
#   make install -- --release
#   make build -- --release
# Dist (any of):
#   make install PROFILE=dist
#   make install -- --dist
# Feature flags via residual goals (any platform):
#   make install -- --features metal      (macOS GPU; also --features cuda on Linux)
# Note: `make install --release` is rejected by GNU make (unknown option). Use `-- --release`.
# Do not use residual goal `release` — it collides with the cross `release` target.
_RELEASE_REQUESTED :=
_DIST_REQUESTED :=
ifneq ($(filter 1 true yes,$(RELEASE)),)
  _RELEASE_REQUESTED := 1
endif
ifneq ($(filter release,$(PROFILE)),)
  _RELEASE_REQUESTED := 1
endif
ifneq ($(filter dist,$(PROFILE)),)
  _DIST_REQUESTED := 1
endif
ifneq ($(filter --release,$(MAKECMDGOALS) $(_RESIDUAL_)),)
  _RELEASE_REQUESTED := 1
endif
ifneq ($(filter --dist,$(MAKECMDGOALS) $(_RESIDUAL_)),)
  _DIST_REQUESTED := 1
endif
# Explicit `--features <name>` residual overrides the auto-detected default.
_AFTER_FEATURES := $(call _after,$(MAKECMDGOALS),--features)
ifneq ($(strip $(_AFTER_FEATURES)),)
  ELPH_METAL_FEATURE := --features $(firstword $(_AFTER_FEATURES))
endif

ifeq ($(_DIST_REQUESTED),1)
  CARGO_BUILD_FLAGS := --profile dist
  BUILD_PROFILE     := dist
else ifeq ($(_RELEASE_REQUESTED),1)
  CARGO_BUILD_FLAGS := --release
  BUILD_PROFILE     := release
else
  CARGO_BUILD_FLAGS :=
  BUILD_PROFILE     := debug
endif
BUILD_DIR := ./target/$(BUILD_PROFILE)

.PHONY: build build-elph install run watch test test-elph test-elph-tui check-elph check-elph-tui generate-models prepare
.PHONY: lint lint-elph lint-elph-tui fmt clean check coverage help stats
.PHONY: cross cross-pull release release-linux release-macos release-windows
.PHONY: bump bump-elph bump-libs publish publish-dry-run version

# ─── Build ──────────────────────────────────────────────────────────────────

check: ## Check code compiles (fast, no codegen)
	@$(CARGO) check --workspace 2>&1
# 	@$(CARGO) bloat --release -n 50

build: build-elph ## Build elph binary (debug default; RELEASE=1 or -- --release)

build-elph: ## Build elph binary (debug default; RELEASE=1 or -- --release)
	@_rustc_display=$$(if [ -n "$$RUSTC_WRAPPER" ]; then echo "$$RUSTC_WRAPPER"; else echo "rustc"; fi); \
	echo "Building $(APP_BIN) v$(ELPH_VERSION) ($(BUILD_HASH)) [$$_rustc_display] ($(BUILD_PROFILE))"
	@_start=$$(python3 -c "import time; print(int(time.time()*1000))"); \
	$(CARGO) build $(CARGO_BUILD_FLAGS) $(ELPH_METAL_FEATURE) --bin $(APP_BIN) 2>&1; \
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
	    echo "Binary $$bin: $$(du -sh $(BUILD_DIR)/$$bin | cut -f1) ($$hash)"; \
	  else \
	    echo "Binary $$bin:(not built)"; \
	  fi; \
	done; \
	printf "Build time:  %d.%03ds\n" $$(( _elapsed / 1000 )) $$(( _elapsed % 1000 ))

install: build ## Install elph (debug -> elph-debug; release -> elph-canary; dist -> elph)
	@mkdir -p $(INSTALL_DIR) && echo
	@for bin in $(APP_BINS); do \
	  if [ "$(BUILD_PROFILE)" = "dist" ]; then \
	    _suffix=""; \
	  else if [ "$(BUILD_PROFILE)" = "release" ]; then \
	    _suffix="-canary"; \
	  else \
	    _suffix="-debug"; \
	  fi; fi; \
      rm -f "$(INSTALL_DIR)/$$bin$${_suffix}"; \
	  cp "$(BUILD_DIR)/$$bin" "$(INSTALL_DIR)/$$bin$${_suffix}"; \
	  echo "$$bin$${_suffix} installed at: $(INSTALL_DIR)/$$bin$${_suffix} [$(BUILD_PROFILE)]"; \
	done

run: ## Run elph coding agent
	@_args='$(or $(_RESIDUAL_),$(ARGS))'; \
	if [ -n "$$_args" ]; then \
		$(CARGO) run -q -p $(APP_BIN) -- $$_args; \
	else \
		$(CARGO) run -q -p $(APP_BIN); \
	fi

watch: ## Run elph with hot reload (requires watchexec)
	@-$(CARGO) watch -c -- cargo run --bin $(APP_BIN) $(or $(_RESIDUAL_),$(ARGS)) 2>&1

test: ## Run all workspace tests
	@$(CARGO) nextest run --no-fail-fast $(or $(_RESIDUAL_),$(ARGS))

test-elph: ## Run tests for elph and its workspace deps
	@$(CARGO) nextest run --no-fail-fast -p elph-ai -p elph $(ARGS)
	@$(CARGO) nextest run --no-fail-fast -p elph-agent --features full $(ARGS)

test-elph-tui: ## Run elph-tui tests
	@$(CARGO) nextest run --no-fail-fast -p elph-tui $(ARGS)

check-elph: ## Check elph and its workspace deps compile
	@$(CARGO) check -p elph-ai -p elph 2>&1
	@$(CARGO) check -p elph-agent --features full --all-targets 2>&1

check-elph-tui: ## Check elph-tui compiles (lib, tests, examples)
	@$(CARGO) check -p elph-tui --all-targets 2>&1

generate-models: ## Regenerate elph-ai model catalogs (pi packages/ai; ARGS=--skip-scripts)
	@$(CARGO) run -p elph-ai --bin generate-models -- all $(ARGS)
	@pnpm dlx --silent oxfmt crates/elph-ai/models/

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
	@$(CARGO) clippy -p elph -p elph-ai --all-targets -- -D warnings
	@$(CARGO) clippy -p elph-agent --features full --all-targets -- -D warnings

lint-elph-tui: ## Run clippy for elph-tui
	@$(CARGO) clippy -p elph-tui --all-targets -- -D warnings

fmt: ## Format all code
	@$(CARGO) fmt --all -- --style-edition 2024
	@pnpm dlx --silent oxfmt crates/elph-ai/models/
	@pnpm dlx --silent oxfmt openwiki/ schemas/

coverage: ## Run tests with coverage (requires cargo-llvm-cov)
	@$(CARGO) llvm-cov nextest --no-cfg-coverage 2>&1

stats: ## Show sccache stats, code line count, and target/ breakdown
	@tokei . -e "*.json" -e "*.md"
	@if [ -n "$(SCCACHE_BIN)" ]; then \
	  echo ""; \
	  printf '\033[33msccache stats:\033[0m\n'; \
	  "$(SCCACHE_BIN)" --show-stats; \
	fi
	@echo ""; \
	printf '\033[33mtarget/breakdown:\033[0m\n'; \
	for _d in debug release; do \
	  if [ -d "target/$$_d" ]; then \
	    _sz=$$(du -sh "target/$$_d" 2>/dev/null | cut -f1); \
	    _incr=$$(du -sh "target/$$_d/incremental" 2>/dev/null | cut -f1); \
	    _deps=$$(du -sh "target/$$_d/deps" 2>/dev/null | cut -f1); \
	    _bins=$$(du -sh "target/$$_d/bin" 2>/dev/null | cut -f1); \
	    printf '  %8s  %8s (incr %s · deps %s · bin %s)\n' "$$_d" "$$_sz" "$$_incr" "$$_deps" "$$_bins"; \
	  fi; \
	done
	@echo ""; \
	printf '\033[33mcargo registry:\033[0m  '; du -sh ~/.cargo/registry/src 2>/dev/null | cut -f1; true

clean: ## Clean build artifacts and caches
	@find crates -type f -name '*_gen.rs' -delete
	@rm -fr crates/elph-ai/models/.cache
	@$(CARGO) clean

# ─── Space reclamation ──────────────────────────────────────────────────────────
#
# Incremental build dirs are redundant with sccache's remote cache — a fresh
# full compile produces the same artifact and re-caches it.  Removing old ones
# is safe; the first rebuild after GC may be slower but re-populates the cache.
#
# Dry-run  (make gc DRY=1): prints what would be deleted without removing anything.
# Defaults: incremental dirs >7 days old, deps files >60 days old.

GC_INCR_MAX_AGE := 7d
GC_DEPS_MAX_AGE  := 60d

gc: ## Reclaim space from stale build artefacts (incremental >7d, deps >60d)
	@_dry="$(DRY)"; \
	if [ "$$_dry" = "1" ]; then _op="would remove"; _skip=yes; else _op="removing"; _skip=no; fi; \
	echo "=== GC $$_op ==="; \
	incr=$$(du -sh target/debug/incremental 2>/dev/null | cut -f1); \
	incr_count=$$(find target/debug/incremental -maxdepth 1 -type d -mtime +$(GC_INCR_MAX_AGE) 2>/dev/null | wc -l | tr -d ' '); \
	echo "  incremental : $$incr ($$incr_count dirs older than $(GC_INCR_MAX_AGE))"; \
	if [ "$$_skip" = "no" ]; then \
	  find target/debug/incremental -maxdepth 1 -type d -mtime +$(GC_INCR_MAX_AGE) -exec rm -rf {} + 2>/dev/null || true; \
	fi; \
	deps=$$(du -sh target/debug/deps 2>/dev/null | cut -f1); \
	dep_count=$$(find target/debug/deps -maxdepth 1 -type f -mtime +$(GC_DEPS_MAX_AGE) 2>/dev/null | wc -l | tr -d ' '); \
	echo "  deps        : $$deps ($$dep_count files older than $(GC_DEPS_MAX_AGE))"; \
	if [ "$$_skip" = "no" ] && [ "$$dep_count" -gt 0 ]; then \
	  find target/debug/deps -maxdepth 1 -type f -mtime +$(GC_DEPS_MAX_AGE) -delete 2>/dev/null || true; \
	fi; \
	echo "  done."; \
	new_inc=$$(du -sh target/debug/incremental 2>/dev/null | cut -f1); \
	new_deps=$$(du -sh target/debug/deps 2>/dev/null | cut -f1); \
	echo "  after: incremental $$new_inc · deps $$new_deps"

clean-incremental: ## Remove all incremental build dirs (faster rebuild, re-cached by sccache)
	@echo "Removing target/debug/incremental/ ..."
	@rm -fr target/debug/incremental
	@echo "Done."

clean-deps: ## Remove deps older than $(GC_DEPS_MAX_AGE) (safe — cargo will re-download/rebuild)
	@echo "Removing deps older than $(GC_DEPS_MAX_AGE) ..."
	@find target/debug/deps -maxdepth 1 -type f -mtime +$(GC_DEPS_MAX_AGE) -print -delete
	@echo "Done."

# ─── Misc ───────────────────────────────────────────────────────────────────

prepare: ## Install required toolchain
	@command -v cargo-binstall >/dev/null 2>&1 || $(CARGO) install cargo-binstall --locked
	@command -v cargo-bloat >/dev/null 2>&1 || $(CARGO) binstall --locked -y cargo-bloat
	@command -v cargo-deny >/dev/null 2>&1 || $(CARGO) binstall --locked -y cargo-deny
	@command -v cargo-nextest >/dev/null 2>&1 || $(CARGO) binstall --locked -y cargo-nextest
	@command -v cargo-machete >/dev/null 2>&1 || $(CARGO) binstall --locked -y cargo-machete
	@command -v cargo-flamegraph >/dev/null 2>&1 || $(CARGO) binstall --locked -y flamegraph
	@command -v cargo-llvm-cov >/dev/null 2>&1 || $(CARGO) binstall --locked -y cargo-llvm-cov
	@command -v communique >/dev/null 2>&1 || $(CARGO) binstall --locked -y communique
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
#   bump-elph  — crates/coding-agent/Cargo.toml
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

_LIBS := elph-ai elph-agent elph-swarm

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
	$(call _bump_manifest,crates/coding-agent/Cargo.toml,$(_BUMP_LEVEL))
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

publish: ## Publish to crates.io (elph-ai first, then libs, then apps)
	@CARGO="$(CARGO)" ./scripts/publish-crates.sh

publish-dry-run: ## Dry-run publish checks (elph-ai first)
	@DRY_RUN=1 CARGO="$(CARGO)" ./scripts/publish-crates.sh

# ─── Help ───────────────────────────────────────────────────────────────────

help: ## Show this help
	@printf '\033[33mUsage:\033[0m make \033[36m<target>\033[0m\n'
	@awk -F ':.*## ' '/^[a-zA-Z_-]+:.*## / {printf " \033[36m%-18s\033[0m %s\n", $$1, $$2}' $(MAKEFILE_LIST)
	@printf '\033[33mBuild profile (build / install):\033[0m\n'
	@printf ' \033[36mmake install\033[0m                      debug -> elph-debug\n'
	@printf ' \033[36mmake install RELEASE=1\033[0m            release -> elph-canary\n'
	@printf ' \033[36mmake install PROFILE=dist\033[0m         dist -> elph\n'
	@printf ' \033[36mmake install -- --release\033[0m         release (GNU make end-of-options)\n'
	@printf ' \033[36mmake install -- --dist\033[0m            dist (GNU make end-of-options)\n'
	@printf ' \033[36mmake install -- --features metal\033[0m  enable metal feature (GPU)\n'
	@printf ' \033[36mmake build PROFILE=release\033[0m        same as RELEASE=1\n'
	@printf '\033[33mNote: \033[36mmake install --release\033[0m is invalid (make option parse)\n'
