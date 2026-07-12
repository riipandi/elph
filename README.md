# Elph

Rust workspace for AI agent applications — coding agent, personal assistant, and documentation tooling.

> [!WARNING]
> This project is under active development, so you may encounter bugs.<br/>
> Please review the release notes thoroughly before updating, as breaking changes can occur!

## Quick Start

Releases are tagged per app (`elph-v*`, `eclaw-v*`, `owly-v*`).
See [GitHub Releases](https://github.com/riipandi/elph/releases).

### Installation

**Pre-built binaries** (Linux and macOS, x86_64 and arm64):

```sh
curl -fsSL https://elph.space/elph/install.sh | bash
curl -fsSL https://elph.space/eclaw/install.sh | bash
curl -fsSL https://elph.space/owly/install.sh | bash
```

Pin a version or install the latest pre-release:

```sh
curl -fsSL https://elph.space/elph/install.sh | bash -s -- --version 0.0.26
curl -fsSL https://elph.space/elph/install.sh | bash -s -- --canary
```

Install scripts resolve the **latest release per application** from GitHub (not the repo-wide
`Latest` badge).

**From crates.io** (requires [Rust >= 1.97][rust]):

```sh
cargo install --locked elph
cargo install --locked eclaw
cargo install --locked owly
```

**From source:**

```sh
cargo install --path elph
cargo install --path eclaw
cargo install --path owly
```

Per-app setup and usage: see each package README linked above.

## Development

Requires [Rust >= 1.97][rust]. Clone the repo, run `make prepare`, then `make help` for targets.

```sh
git clone https://github.com/riipandi/elph.git
cd elph
make prepare
make check && make test && make lint
```

Publish crates: `make publish` (see `make help`)

## Contributing

We welcome contributions to make Elph even better!

- Read our **[Contributing Guidelines](./CONTRIBUTING.md)** for detailed guidelines
- Fork the repository and create a feature branch
- Submit a pull request with a clear title and description
- Join the discussion on [GitHub Issues](https://github.com/riipandi/elph/issues)

Join the flow. Amplify your AI-powered workflow with Elph! 🚀

## Documentation

Documentation lives in [`docs/`](./docs/). Start with [docs/README.md](./docs/README.md).

## Attribution

Elph re-implements concepts from several open-source projects in Rust:

- **[pi](https://pi.dev)** by Mario Zechner — architectural design, provider abstraction, tool system (MIT).
- **[OpenAI Codex CLI](https://github.com/openai/codex)** — Agent workflow inspiration: exit summary, goals, subagent orchestration (Apache 2.0).
- **[memelord](https://github.com/glommer/memelord)** by Glauber Costa — `floppy` memory module port (MIT).
- **[code-review-graph](https://github.com/tirth8205/code-review-graph)** by Tirth Kanani — `elph codegraph` integration (MIT).
- **[OpenWiki](https://github.com/langchain-ai/openwiki)** by LangChain — `owly` crate port (MIT).

See [NOTICE.md](./NOTICE.md) for details and license texts.

## License

This workspace uses a mixed license model:

- **Applications** (`elph`, `eclaw`, `owly`) — [Apache License 2.0][license-apache] ([LICENSE-APACHE](./LICENSE-APACHE))
- **Libraries** (`elph-core`, `elph-ai`, `elph-agent`, `elph-tui`, `elph-swarm`) — [MIT License][license-mit] ([LICENSE-MIT](./LICENSE-MIT))

Third-party attributions and upstream license requirements are listed in [NOTICE.md](./NOTICE.md).

---

<sub>🤫 Psst! If you like my work you can support me via [GitHub sponsors](https://github.com/sponsors/riipandi).</sub>

[![CreatorBadge](https://badgen.net/badge/icon/Aris%20Ripandi?label=Made+by&color=black&labelColor=black)](https://x.com/intent/follow?screen_name=riipandi)

<!-- References -->

[rust]: https://rust-lang.org/tools/install/
[license-apache]: https://www.tldrlegal.com/license/apache-license-2-0-apache-2-0
[license-mit]: https://www.tldrlegal.com/license/mit-license
