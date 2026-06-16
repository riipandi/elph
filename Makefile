.DEFAULT_GOAL := help

GO          := $$(which go)
GOTEST      := $$(which gotestsum)

BINARY_NAME := elph
BUILD_DIR   := ./build/release
PKG_NAME    := github.com/riipandi/elph
PKG_VERSION := 0.0.0
BUILD_HASH  := $$(git rev-parse --short HEAD)
BUILD_DATE  := $$(date -u +"%Y-%m-%dT%H:%M:%SZ")
BUILD_META  := -X $(PKG_NAME)/internal/config.AppVersion=$(PKG_VERSION) \
							-X $(PKG_NAME)/internal/config.BuildHash=$(BUILD_HASH) \
							-X $(PKG_NAME)/internal/config.BuildDate=$(BUILD_DATE)

# ─── Args / Flags ────────────────────────────────────────────────────────────

# Named args:  make run ARGS="--port 8080"  /  make test PKG=./internal/...
ARGS :=
PKG  ?= ./...

# Positional args:  make test ./...  (captures words after the target)
_RESIDUAL_ := $(wordlist 2,$(words $(MAKECMDGOALS)),$(MAKECMDGOALS))
$(foreach a,$(_RESIDUAL_),$(eval .PHONY: $a))
$(foreach a,$(_RESIDUAL_),$(eval $a: ; @true))

# ─── Test Flags ──────────────────────────────────────────────────────────────

TEST_FLAGS := --format short-verbose -- -count=1 -v

.PHONY: build run install
.PHONY: test integration coverage
.PHONY: clean prepare deps lint fmt vet help

# ─── Build ───────────────────────────────────────────────────────────────────

build: ## Build the application binary
	@echo "Building $(PKG_NAME) v$(PKG_VERSION) ($(BUILD_HASH)) $(BUILD_DATE)"
	@_start=$$(python3 -c "import time; print(int(time.time()*1000))"); \
	$(GO) build -ldflags="-w -s -extldflags -static-pie $(BUILD_META)" -o $(BUILD_DIR)/$(BINARY_NAME) ./cmd/elph; \
	_end=$$(python3 -c "import time; print(int(time.time()*1000))"); \
	_elapsed=$$(( _end - _start )); \
	echo "Binary size: $$(du -sh $(BUILD_DIR)/$(BINARY_NAME) | cut -f1) ($$(shasum -a 1 $(BUILD_DIR)/$(BINARY_NAME) | cut -d' ' -f1))"; \
	echo "Binary file: $(BUILD_DIR)/$(BINARY_NAME)"; \
	printf "Build time:  %d.%03ds\n" $$(( _elapsed / 1000 )) $$(( _elapsed % 1000 ))

install: build ## Build and copy binary to ~/.local/bin
	@mkdir -p $(HOME)/.local/bin
	@cp $(BUILD_DIR)/$(BINARY_NAME) $(HOME)/.local/bin/$(BINARY_NAME)
	@echo "Installed: $$(command -v $(HOME)/.local/bin/$(BINARY_NAME) 2>/dev/null || echo $(HOME)/.local/bin/$(BINARY_NAME))"

run: ## Run the application
	@$(GO) run ./cmd/elph $(or $(_RESIDUAL_),$(ARGS))

# ─── Testing ─────────────────────────────────────────────────────────────────

test: ## Run unit tests
	@$(GOTEST) $(TEST_FLAGS) $(or $(addprefix ./,$(_RESIDUAL_)),$(PKG))

integration: ## Run integration tests
	@$(GOTEST) $(TEST_FLAGS) -tags=integration ./internal/...

coverage: ## Run tests with coverage report
	@$(GOTEST) ./internal/... ./pkg/... -v -coverprofile=coverage.out
	@$(GO) tool cover -html=coverage.out -o coverage.html

# ─── Code Quality ────────────────────────────────────────────────────────────

lint: ## Run linter (requires golangci-lint)
	@golangci-lint run ./...

fmt: ## Format code
	@$(GO) fmt ./...

vet: ## Analyzes code for suspicious patterns
	@$(GO) vet ./...

# ─── Maintenance ─────────────────────────────────────────────────────────────

prepare: ## Install required toolchain
	$(GO) install github.com/golangci/golangci-lint/v2/cmd/elph/golangci-lint@latest
	$(GO) install github.com/pressly/goose/v3/cmd/elph/goose@latest
	$(GO) install gotest.tools/gotestsum@latest

deps: ## Download dependencies
	@$(GO) mod download
	@$(GO) mod tidy

clean: ## Clean build artifacts
	@rm -rf $(BUILD_DIR) vendor node_modules coverage.out coverage.html
	@find internal -type f -name '*_gsx.go' -delete

# ─── Website ─-----───────────────────────────────────────────────────────────

site-dev: ## Run website in development
	@hugo server --config docsite/hugo.yaml

site-build: ## Build the documentation site
	@hugo build --config docsite/hugo.yaml --gc --minify

# ─── Help ────────────────────────────────────────────────────────────────────

help: ## Show this help
	@printf '\033[33mUsage:\033[0m make \033[36m<target>\033[0m\n'
	@awk -F ':.*## ' '/^[a-zA-Z_-]+:.*## / {printf " \033[36m%-18s\033[0m %s\n", $$1, $$2}' $(MAKEFILE_LIST)
