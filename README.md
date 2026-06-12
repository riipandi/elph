# Elph - minimalist AI agent companion

> [!WARNING]
> This project is under active development, so you may encounter bugs.<br/>
> Please review the release notes thoroughly before updating, as breaking changes can occur!

## Quick Start

You will need [`Go >=1.26`][golang], [`Node.js >= 24.15`][nodejs], [`PNPM >= 11.5`][pnpm],
and [`Docker >= 20.10`][docker] installed on your machine.

Also, you need to install the following tools:

```sh
go install github.com/golangci/golangci-lint/v2/cmd/golangci-lint@latest
go install github.com/grindlemire/go-tui/cmd/tui@latest
go install github.com/pressly/goose/v3/cmd/goose@latest
go install gotest.tools/gotestsum@latest
```

Read the [CONTRIBUTING.md](./CONTRIBUTING.md) for detailed guidelines on contributing to this project.

### Up and Running

```sh
# Clone the repository
git clone <repository-url>
cd elph

# Install dependencies
make deps

# Run the application
make run
```

## Documentation

For more detailed information about the system architecture, design decisions, and project structure,
please refer to the documentation in the [`docs`](./docs) directory.

## License

This project licensed under the [MIT license][license-mit]. See the [LICENSE](./LICENSE) file for more information.

---

<sub>🤫 Psst! If you like my work you can support me via [GitHub sponsors](https://github.com/sponsors/riipandi).</sub>

[![Made by](https://badgen.net/badge/icon/Aris%20Ripandi?label=Made+by&color=black&labelColor=black)](https://x.com/intent/follow?screen_name=riipandi)

<!-- References -->
[docker]: https://docs.docker.com/engine/install/
[golang]: https://go.dev/doc/install
[license-mit]: https://choosealicense.com/licenses/mit/
[nodejs]: https://nodejs.org/en/download
[pnpm]: https://pnpm.io/installation
