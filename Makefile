.PHONY: all build test fmt clippy wasm js demo clean

all: build test

build:
	cargo build --workspace

test:
	cargo test --workspace

fmt:
	cargo fmt --all

clippy:
	cargo clippy --workspace --all-targets -- -D warnings

wasm:
	wasm-pack build ffi/wasm --target web --release --out-dir ../../js/wasm

js: wasm
	cd js && npm install && npm run build:ts

demo: js
	cd demo && npm install && npm run build

dev:
	cd demo && npm run dev

clean:
	cargo clean
	rm -rf js/dist js/wasm demo/dist js/node_modules demo/node_modules
