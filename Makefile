.PHONY: build release install lint test clean

build:
	cargo build

release:
	cargo build --release

install: release
	cp target/release/shelly-cli ~/.local/bin/shelly-cli

lint:
	cargo clippy -- -D warnings

test:
	cargo test

clean:
	cargo clean
