# Elph Example Extension

WASM component extension (wasmtime + Component Model) that adds `/say-hello <name>`.

## Build

Requires the [cargo-component](https://github.com/bytecodealliance/cargo-component) subcommand:

```sh
cargo install cargo-component
rustup target add wasm32-wasip2
cd extensions/say-hello
cargo component build --release
cp target/wasm32-wasip2/release/elph_extension_say_hello.wasm component.wasm
```

## Install

```sh
elph plugin install extensions/say-hello --force
```

## Usage

```
/say-hello John Doe
```