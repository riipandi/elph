# ACP Registry entry

The canonical manifest is `crates/coding-agent/agent.json`. `docs/acp-registry/elph/`
holds the icon and this README, which are copied alongside it when opening a
registry PR.

Registry CI requires at least one `authMethods` entry of `type: agent` or
`type: terminal`. Elph advertises both: Terminal Auth (`elph acp --setup`) and
agent methods (`existing-credentials` plus provider ids).

## Submit

1. Publish a GitHub Release whose archives match `crates/coding-agent/agent.json` (`archive` URLs and `sha256`).
2. Fork the registry repo.
3. Assemble `<fork>/elph/`: copy `crates/coding-agent/agent.json` as `agent.json`,
   and `docs/acp-registry/elph/icon.svg` (directory name must equal `id`).
4. Open a PR. Local check (in the registry clone):

```sh
python3 .github/workflows/verify_agents.py --auth-check --agent elph
```

Do not use `/latest/` in archive URLs. After merge, the registry updates versions from GitHub Releases hourly.

## Platform key → release asset

Registry platform keys are fixed by
[`agent.schema.json`](https://cdn.agentclientprotocol.com/registry/v1/latest/agent.schema.json)
(`darwin-aarch64`, `darwin-x86_64`, `linux-aarch64`, `linux-x86_64`,
`windows-aarch64`, `windows-x86_64`). They do **not** match Elph's release asset labels, so keep this mapping in sync with `.github/workflows/release.yml`:

| Registry key     | Release asset               |
| ---------------- | --------------------------- |
| `darwin-aarch64` | `elph-macos-aarch64.tar.gz` |
| `darwin-x86_64`  | `elph-macos-x86_64.tar.gz`  |
| `linux-aarch64`  | `elph-linux-arm64.tar.gz`   |
| `linux-x86_64`   | `elph-linux-x86_64.tar.gz`  |
| `windows-x86_64` | `elph-windows-x86_64.zip`   |

`windows-aarch64` is not built. Missing platforms inside an OS family are allowed;
a missing OS family only produces a validation warning.

## Version and checksum upkeep

- `version` must be strict `x.y.z` with numeric parts. It is the version the
  release binary reports in `initialize` (`ci-set-app-version.sh` pins
  `crates/coding-agent/Cargo.toml` at release time; the in-repo `0.0.0` is a
  placeholder and must not be copied here).
- Archive URLs use the **git tag**, which currently carries a `-canary` suffix
  (`v0.0.5-canary`) while `version` stays numeric (`0.0.5`).
- Refresh every `sha256` from the release's `SHA256SUMS` asset on each bump:

```sh
tag=v0.0.5-canary
curl -sSL "https://github.com/riipandi/elph/releases/download/${tag}/SHA256SUMS"
```

**Caveat:** all releases so far are GitHub _prereleases_. The registry's hourly
auto-update reads the latest release tag, and GitHub excludes prereleases from
`/releases/latest`, so version bumps need a manual PR until a non-prerelease is
published.

## Handshake args

| Flow          | Args          |
| ------------- | ------------- |
| Normal ACP    | `acp --stdio` |
| Terminal Auth | `acp --setup` |
