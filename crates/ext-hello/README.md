# Elph Example Extension

Core Wasm extension (wasmi) that adds `/say-hello <name>` and can block `shell_exec` commands containing `rm -rf` (confirm defaults to deny without a TUI).

## Build

```sh
rustup target add wasm32-unknown-unknown
cd crates/ext-hello
cargo build --release --target wasm32-unknown-unknown
cp target/wasm32-unknown-unknown/release/elph_extension_say_hello.wasm plugin.wasm
```

## Install

```sh
elph extensions install crates/ext-hello --force
```

## Usage

```
/say-hello John Doe
```
