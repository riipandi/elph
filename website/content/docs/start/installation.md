# Installation

Pre-built binaries for Linux and macOS (x86_64 and arm64):

```sh
curl -fsSL https://elph.space/install.sh | bash
```

Pin a version or install the latest pre-release:

```sh
curl -fsSL https://elph.space/install.sh | bash -s -- --version 0.0.26
curl -fsSL https://elph.space/install.sh | bash -s -- --canary
```

From crates.io (Rust ≥ 1.97):

```sh
cargo install --locked elph
```

Verify:

```sh
elph --version
```

## First launch

```sh
elph
```

On first run Elph scaffolds config, data, and a project `.elph/` directory. Provider catalogs unpack into `CONFIG_DIR/providers/`. Built-in skills and the user guide land under `CONFIG_DIR/bundled/`.

Then set credentials — see [Providers](/docs/start/providers).
