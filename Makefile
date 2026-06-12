# Show help by default when `make` is run without arguments
.DEFAULT_GOAL := help

.PHONY: build run test test-unit test-integration test-coverage clean

# Build variables
BINARY_NAME=elph
BUILD_DIR=./build/release

# Go parameters
GOCMD=go
GOBUILD=$(GOCMD) build
GOGET=$(GOCMD) get
GOMOD=$(GOCMD) mod
TUICMD=$(shell which tui)

# Build the application
build:
	$(GOBUILD) -o $(BUILD_DIR)/$(BINARY_NAME) ./cmd
	@echo "Binary size: $$(du -sh $(BUILD_DIR)/$(BINARY_NAME) | cut -f1)  ($$(wc -c < $(BUILD_DIR)/$(BINARY_NAME) | tr -d ' ') bytes)"

# Run the application
run:
	$(TUICMD) generate internal/views/
	$(GOCMD) run ./cmd

# Run all tests
test:
	gotestsum --format short-verbose -- -count=1 -v ./...

# Run unit tests only
test-unit:
	gotestsum --format short-verbose -- -count=1 -v ./internal/adapter/... ./internal/service/... ./internal/handler/...

# Run integration tests
test-integration:
	gotestsum --format short-verbose -- -tags=integration -count=1 -v ./internal/integration/...

# Run tests with coverage
test-coverage:
	gotestsum ./internal/... -v -coverprofile=coverage.out
	$(GOCMD) tool cover -html=coverage.out -o coverage.html

# Clean build artifacts
clean:
	rm -rf $(BUILD_DIR)
	rm -f coverage.out coverage.html

# Download dependencies
deps:
	$(GOMOD) download
	$(GOMOD) tidy

# Lint (requires golangci-lint)
lint:
	golangci-lint run ./...

# Format code
fmt:
	$(GOCMD) fmt ./...

# Vet code
vet:
	$(GOCMD) vet ./...

# Show help
help:
	@echo "Available targets:"
	@echo "  build             - Build the application"
	@echo "  run               - Run the application"
	@echo "  test              - Run all tests"
	@echo "  test-unit         - Run unit tests only"
	@echo "  test-integration  - Run integration tests"
	@echo "  test-coverage     - Run tests with coverage report"
	@echo "  clean             - Clean build artifacts"
	@echo "  deps              - Download dependencies"
	@echo "  lint              - Run linter"
	@echo "  fmt               - Format code"
	@echo "  vet               - Vet code"
	@echo "  help              - Show this help"
