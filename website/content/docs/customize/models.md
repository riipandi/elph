# Custom models

Elph embeds provider catalogs and merges disk overlays from `CONFIG_DIR/providers/`. Generated and unpacked files stamp `"$schema": "https://elph.space/provider-schema.json"`.

## File shapes

1. **Map** — `modelId → model` object (unpacked default).
2. **Schema wrapper**:

```json
{
  "$schema": "https://elph.space/provider-schema.json",
  "baseUrl": "https://gateway.example",
  "headers": { "X-Custom": "1" },
  "models": {
    "my-model": {
      "id": "my-model",
      "name": "My Model",
      "api": "openai-completions",
      "provider": "custom",
      "baseUrl": "",
      "reasoning": false,
      "input": ["text"],
      "cost": { "input": 1, "output": 1, "cacheRead": 0, "cacheWrite": 0 },
      "contextWindow": 128000,
      "maxTokens": 8192
    }
  }
}
```

- File stem = provider id (kebab-case).
- Disk models replace embedded models with the same `id`.
- Unknown provider ids from disk become custom providers.
- Existing provider JSON files are **never** overwritten on bootstrap.

```sh
elph models
elph provider
```

Limit the picker / catalog with `models.enabled` in `settings.json` (globs on `provider/model_id` or bare id). `models.scopedModels` remains the explicit Ctrl+P list and is not stripped by the glob.

```json
{
  "models": {
    "enabled": ["openai/*", "anthropic/claude-*", "!*-preview"]
  }
}
```
