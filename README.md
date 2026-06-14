# Elph - minimalist AI agent companion

> [!WARNING]
> This project is under active development, so you may encounter bugs.<br/>
> Please review the release notes thoroughly before updating, as breaking changes can occur!

## Quick Start

You will need [`Go >= 1.26`][golang] installed. Optional: `gotestsum` and `golangci-lint` (installed via `make prepare`).

Read the [CONTRIBUTING.md](./CONTRIBUTING.md) for detailed guidelines on contributing to this project.

### Up and Running

```sh
# Clone the repository
git clone <repository-url>
cd elph

# Install required toolchain
make prepare

# Install dependencies
make deps

# Run the application
make run
```

## Documentation

Documentation lives in [`docs/`](./docs/). Start with [docs/README.md](./docs/README.md).

## License

This project licensed under the [MIT license][license-mit]. See the [LICENSE](./LICENSE) file for more information.

---

<sub>🤫 Psst! If you like my work you can support me via [GitHub sponsors](https://github.com/sponsors/riipandi).</sub>

[![Made by](https://badgen.net/badge/icon/Aris%20Ripandi?label=Made+by&color=black&labelColor=black)](https://x.com/intent/follow?screen_name=riipandi)

<!-- References -->
[golang]: https://go.dev/doc/install
[license-mit]: https://choosealicense.com/licenses/mit/
