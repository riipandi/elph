# ACP Registry entry

Files in `elph/` are the payload for a pull request to
[agentclientprotocol/registry](https://github.com/agentclientprotocol/registry).

Registry CI requires at least one `authMethods` entry of `type: agent` or
`type: terminal`. Elph advertises both: Terminal Auth (`elph acp --setup`) and
agent methods (`existing-credentials` plus provider ids).

## Submit

1. Publish a GitHub Release whose archives match `elph/agent.json` (`archive` URLs and `sha256`).
2. Fork the registry repo.
3. Copy `docs/acp-registry/elph/` to `<fork>/elph/` (directory name must equal `id`).
4. Open a PR. Local check (in the registry clone):

```sh
python3 .github/workflows/verify_agents.py --auth-check --agent elph
```

Do not use `/latest/` in archive URLs. After merge, the registry updates versions from GitHub Releases hourly.

## Handshake args

| Flow | Args |
|---|---|
| Normal ACP | `acp --stdio` |
| Terminal Auth | `acp --setup` |
