.PHONY: build test clippy alpine clean

# Default target
all: build

# Build the project in debug mode
build:
	cargo build

# Run tests
test:
	cargo test

# Run clippy for linting with all features
clippy:
	cargo clippy --all-features -- -D warnings

# Build for Alpine Linux (musl target)
alpine:
	cargo build --target x86_64-unknown-linux-musl

# Clean build artifacts
clean:
	cargo clean

# Install the musl target if it doesn't exist
install-musl-target:
	rustup target add x86_64-unknown-linux-musl

# Build release version
release:
	cargo build --release

# Build release version for Alpine Linux
alpine-release:
	cargo build --release --target x86_64-unknown-linux-musl

# Help message
help:
	@echo "Available targets:"
	@echo "  make          - Build the project in debug mode"
	@echo "  make build    - Build the project in debug mode"
	@echo "  make test     - Run tests"
	@echo "  make clippy   - Run clippy for linting with all features"
	@echo "  make alpine   - Build for Alpine Linux (musl target)"
	@echo "  make clean    - Clean build artifacts"
	@echo "  make release  - Build release version"
	@echo "  make alpine-release - Build release version for Alpine Linux"
	@echo "  make install-musl-target - Install the musl target"
