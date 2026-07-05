# Elph - minimalist AI agent companion

> [!WARNING]
> This project is under active development, so you may encounter bugs.<br/>
> Please review the release notes thoroughly before updating, as breaking changes can occur!

## Quick Start

You will need [`Rust >= 1.96`][rust] installed. Run `make prepare` to install all toolchain dependencies:
`watchexec`, `tarpaulin`, `cross`, and `rustup` targets for cross-compilation.

Read the [CONTRIBUTING.md](./CONTRIBUTING.md) for detailed guidelines on contributing to this project.

### Installation

Install using the [install](./install.sh) script:

```sh
curl -fsSL https://elph.space/install.sh | bash
```

Or use `cargo install` (requires Rust 1.96+):

```sh
cargo install --locked elph
```

### Up and Running

```sh
# Clone the repository
git clone REPOSITORY_URL
cd elph

# Install required toolchain
make prepare

# Install dependencies
make check

# Run the application
make run
```

### Publishing

Publish all crates to crates.io (order matters: core → ai → agent → tui → elph):

```sh
make publish
```

Or publish individually:

```sh
cargo publish -p elph-core
cargo publish -p elph-ai
cargo publish -p elph-agent
cargo publish -p elph-tui
cargo publish -p eclaw
cargo publish -p elph
```

**Note:** crates.io is immutable — once published, a version cannot be overwritten or deleted.

## Documentation

Documentation lives in [`docs/`](./docs/). Start with [docs/README.md](./docs/README.md).

## License

This project licensed under the [MIT license][license-mit]. See the [LICENSE](./LICENSE) file for more information.

---

<sub>🤫 Psst! If you like my work you can support me via [GitHub sponsors](https://github.com/sponsors/riipandi).</sub>

[![Made by](https://badgen.net/badge/icon/Aris%20Ripandi?label=Made+by&color=black&labelColor=black)](https://x.com/intent/follow?screen_name=riipandi)

<!-- References -->

[rust]: https://rust-lang.org/tools/install/
[license-mit]: https://www.tldrlegal.com/license/mit-license
