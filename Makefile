.PHONY: build test fmt clippy bench clean

# Build release binary
build:
	cargo build --release

# Run test suite
test:
	cargo test

# Format code
fmt:
	cargo fmt --all

# Lints (clippy)
clippy:
	cargo clippy --all-targets --all-features -- -D warnings

# Benchmarks (criterion benches live under benches/)
bench:
	cargo bench

clean:
	cargo clean
