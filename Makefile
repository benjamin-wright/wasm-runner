.DEFAULT_GOAL := build

.PHONY: setup update-deps build install check

setup:
	rustup toolchain install

update-deps:
	cargo update

build:
	cargo build --locked

install:
	cargo install --locked --path .

check:
	cargo fmt --check
	cargo clippy --locked --all-targets -- -D warnings
	cargo test --locked
