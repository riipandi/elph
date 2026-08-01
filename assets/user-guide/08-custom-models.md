# Custom Models

Elph embeds provider catalogs and merges disk overlays from `CONFIG_DIR/providers/`.

## File shapes

1. **Map** — `modelId → model` object (unpacked default).
2. **Schema wrapper**:

```json
{
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
            "cost": {
                "input": 1,
                "output": 1,
                "cacheRead": 0,
                "cacheWrite": 0
            },
            "contextWindow": 128000,
            "maxTokens": 8192
        }
    }
}
```

Wrapper `baseUrl` / `headers` apply to models that omit them; per-model values win.

## Behavior

- File stem = provider id (kebab-case).
- Disk models replace embedded models with the same `id`.
- Unknown provider ids from disk become custom providers.
- Existing provider JSON files are **never** overwritten on bootstrap.

```sh
elph models
elph provider
```
