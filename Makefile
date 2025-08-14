# Makefile for homies_gaming_backend

# Variables
CARGO = cargo
TARGET = homies_gaming_backend
RELEASE_TARGET = target/release/$(TARGET)
UPLOADS_DIR = uploads

# Default target
.PHONY: all
all: build

# Build the project
.PHONY: build
build:
	$(CARGO) build

# Build for release
.PHONY: release
release:
	$(CARGO) build --release

# Run the application
.PHONY: run
run:
	$(CARGO) run

# Run the application in release mode
.PHONY: run-release
run-release: $(RELEASE_TARGET)
	./$(RELEASE_TARGET)

# Create uploads directory
.PHONY: setup
setup:
	mkdir -p $(UPLOADS_DIR)

# Clean build artifacts
.PHONY: clean
clean:
	$(CARGO) clean

# Check code without building
.PHONY: check
check:
	$(CARGO) check

# Run tests
.PHONY: test
test:
	$(CARGO) test

# Format code
.PHONY: fmt
fmt:
	$(CARGO) fmt

# Check code formatting
.PHONY: fmt-check
fmt-check:
	$(CARGO) fmt --check

# Run clippy linter
.PHONY: lint
lint:
	$(CARGO) clippy -- -D warnings

# Install dependencies
.PHONY: install-deps
install-deps:
	$(CARGO) fetch

# Start development server with file watching (requires cargo-watch)
.PHONY: watch
watch:
	@command -v cargo-watch >/dev/null 2>&1 || { echo "cargo-watch not installed. Run: cargo install cargo-watch"; exit 1; }
	cargo watch -x run

# Install cargo-watch
.PHONY: install-watch
install-watch:
	$(CARGO) install cargo-watch

# Package the application
.PHONY: package
package: release
	tar -czf $(TARGET).tar.gz $(RELEASE_TARGET) README.md

# Help target
.PHONY: help
help:
	@echo "Available targets:"
	@echo "  all          - Build the project (default)"
	@echo "  build        - Build the project"
	@echo "  release      - Build for release"
	@echo "  run          - Run the application"
	@echo "  run-release  - Run the application in release mode"
	@echo "  setup        - Create uploads directory"
	@echo "  clean        - Clean build artifacts"
	@echo "  check        - Check code without building"
	@echo "  test         - Run tests"
	@echo "  fmt          - Format code"
	@echo "  fmt-check    - Check code formatting"
	@echo "  lint         - Run clippy linter"
	@echo "  install-deps - Install dependencies"
	@echo "  watch        - Start development server with file watching"
	@echo "  install-watch - Install cargo-watch"
	@echo "  package      - Package the application"
	@echo "  help         - Show this help message"

# Ensure uploads directory exists before running
$(RELEASE_TARGET): setup

# Declare phony targets
.PHONY: all build release run run-release clean check test fmt fmt-check lint install-deps watch install-watch package help setup
