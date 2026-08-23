# Installation

Pre-built binaries for Linux, macOS, and Windows (x86_64 and arm64):

```sh
curl -fsSL https://elph.space/install.sh | bash
```

### Windows (x86_64)

Use the PowerShell installer:

```powershell
powershell -ExecutionPolicy Bypass -c "irm https://elph.space/install.ps1 | iex"
```

Pin a version:

```powershell
$env:ELPH_VERSION="0.0.26"; powershell -ExecutionPolicy Bypass -c "irm https://elph.space/install.ps1 | iex"
```

Install the latest pre-release with environment variables:

```powershell
$env:ELPH_CANARY="1"; powershell -ExecutionPolicy Bypass -c "irm https://elph.space/install.ps1 | iex"
```

The binary is installed to `%LOCALAPPDATA%\Programs\elph\bin`; add that directory to your
`PATH` (the installer prints the snippet).

### Linux / macOS

Pin a version:

```sh
curl -fsSL https://elph.space/install.sh | bash -s -- --version 0.0.26
```

Install the latest pre-release:

```sh
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
