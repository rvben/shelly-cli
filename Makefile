.PHONY: build release test lint fmt fmt-check clean install release-patch release-minor release-major

build:
	cargo build

release:
	cargo build --release

test:
	cargo test

lint:
	cargo clippy -- -D warnings

fmt:
	cargo fmt

fmt-check:
	cargo fmt --check

clean:
	cargo clean

install:
	cargo install --path .

release-patch:
	vership bump patch

release-minor:
	vership bump minor

release-major:
	vership bump major
