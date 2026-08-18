# Elph

Rust workspace for AI agent applications — coding agent CLI, shared runtime libraries, and terminal UI components.

> [!WARNING]
> This actively developed project may have usability issues due to ongoing changes and occasional bugs.<br/>
> Always check release notes before upgrading, since breaking changes could disrupt your workflow.

## Quick Start

Releases are tagged as `elph-v*`.
See [GitHub Releases](https://github.com/riipandi/elph/releases).

### Installation

**Pre-built binaries** (Linux and macOS, x86_64 and arm64):

```sh
curl -fsSL https://elph.space/install.sh | bash
```

Pin a version or install the latest pre-release:

```sh
curl -fsSL https://elph.space/install.sh | bash -s -- --version 0.0.26
curl -fsSL https://elph.space/install.sh | bash -s -- --canary
```

**From crates.io** (requires [Rust >= 1.97][rust]):

```sh
cargo install --locked elph
```

**From source:**

```sh
cargo install --path elph
```

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
- **[Grok Build](https://github.com/xai-org/grok-build)** by SpaceXAI — coding-agent system prompt (Apache 2.0).

See [NOTICE.md](./NOTICE.md) for details and license texts.

## License

This workspace uses a mixed license model:

- **Application** (`elph`) — [Apache License 2.0][license-apache] ([LICENSE-APACHE](./LICENSE-APACHE))
- **Libraries** (`elph-ai`, `elph-agent`, `elph-tui`, `elph-swarm`) — [MIT License][license-mit] ([LICENSE-MIT](./LICENSE-MIT))

Third-party attributions and upstream license requirements are listed in [NOTICE.md](./NOTICE.md).

---

<sub>🤫 Psst! If you like my work you can support me via [GitHub sponsors](https://github.com/sponsors/riipandi).</sub>

[![CreatorBadge](https://badgen.net/badge/icon/Aris%20Ripandi?label=Made+by&color=black&labelColor=black)](https://x.com/intent/follow?screen_name=riipandi)

<!-- References -->

[rust]: https://rust-lang.org/tools/install/
[license-apache]: https://www.tldrlegal.com/license/apache-license-2-0-apache-2-0
[license-mit]: https://www.tldrlegal.com/license/mit-license
