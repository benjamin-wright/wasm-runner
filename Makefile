.DEFAULT_GOAL := build

.PHONY: setup update-deps build install check

setup:
	rustup show

update-deps:
	cargo update

build:
	cargo build --locked

install:
	cargo install --locked --path .

check:
	cargo fmt --all --check
	cargo clippy --locked --workspace --all-targets -- -D warnings
	cargo test --locked --workspace
