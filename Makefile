.DEFAULT_GOAL := help

BINARY_NAME := elph
BUILD_DIR   := ./build/release
GO          := go
GOTEST      := gotestsum

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
	@$(GO) build -ldflags="-s -w" -o $(BUILD_DIR)/$(BINARY_NAME) ./cmd
	@echo "Binary size: $$(du -sh $(BUILD_DIR)/$(BINARY_NAME) | cut -f1) ($$(shasum -a 1 $(BUILD_DIR)/$(BINARY_NAME) | cut -d' ' -f1))"

install: build ## Build and copy binary to ~/.local/bin
	@mkdir -p $(HOME)/.local/bin
	@cp $(BUILD_DIR)/$(BINARY_NAME) $(HOME)/.local/bin/$(BINARY_NAME)
	@echo "Installed: $$(command -v $(HOME)/.local/bin/$(BINARY_NAME) 2>/dev/null || echo $(HOME)/.local/bin/$(BINARY_NAME))"

run: ## Run the application
	@$(GO) run ./cmd $(or $(_RESIDUAL_),$(ARGS))

# ─── Testing ─────────────────────────────────────────────────────────────────

test: ## Run unit tests
	@$(GOTEST) $(TEST_FLAGS) $(or $(addprefix ./,$(_RESIDUAL_)),$(PKG))

integration: ## Run integration tests
	@$(GOTEST) $(TEST_FLAGS) -tags=integration ./internal/...

coverage: ## Run tests with coverage report
	@$(GOTEST) ./internal/... -v -coverprofile=coverage.out
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
	$(GO) install github.com/golangci/golangci-lint/v2/cmd/golangci-lint@latest
	$(GO) install github.com/pressly/goose/v3/cmd/goose@latest
	$(GO) install gotest.tools/gotestsum@latest

deps: ## Download dependencies
	@$(GO) mod download
	@$(GO) mod tidy

clean: ## Clean build artifacts
	@rm -rf $(BUILD_DIR) vendor node_modules coverage.out coverage.html
	@find internal -type f -name '*_gsx.go' -delete

# ─── Help ────────────────────────────────────────────────────────────────────

help: ## Show this help
	@printf '\033[33mUsage:\033[0m make \033[36m<target>\033[0m\n'
	@awk -F ':.*## ' '/^[a-zA-Z_-]+:.*## / {printf " \033[36m%-18s\033[0m %s\n", $$1, $$2}' $(MAKEFILE_LIST)
