.PHONY: build
build:
	cargo build

.PHONY: run
run:
	cargo run

.PHONY: test
test:
	cargo test

.PHONY: check
check:
	cargo clippy

.PHONY: fmt
fmt:
	nix fmt

.PHONY: clean
clean:
	cargo clean
