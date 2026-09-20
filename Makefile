.PHONY: check install test

check:
	cargo fmt --all -- --check
	cargo check --locked
	cargo clippy --all-targets --locked -- -D warnings

install:
	cargo install --path . --locked --root $(HOME)/.local

test:
	cargo test --locked
