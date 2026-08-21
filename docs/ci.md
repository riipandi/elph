# CI / CD

GitHub Actions run on [Namespace](https://namespace.so/docs/solutions/github-actions) runners (Linux, macOS) and [Avrea](https://docs.avrea.com/getting-started/) runners (Windows). Compiler artifacts go through [sccache](https://github.com/mozilla/sccache): Namespace WebDAV on Linux/macOS, Avrea WebDAV (`SCCACHE_WEBDAV_ENDPOINT`) on Windows. Cargo registry and `target/` live on a Namespace cache volume (`namespacelabs/nscloud-cache-action`) for Linux/macOS; Windows uses the Avrea GitHub Actions cache proxy (`swatinem/rust-cache`).

### sccache / Namespace

`setup-rust` provisions sccache credentials automatically on Namespace runners via
`nsc cache sccache setup --cache_name default` (WebDAV backend). No R2 secrets or
repository variables are required. The composite sets `RUSTC_WRAPPER=sccache`,
`SCCACHE_DIRECT=true`, and `SCCACHE_MAXSIZE=20G`.

### sccache / Avrea (Windows)

Windows uses `cacheBackend: avrea`: `swatinem/rust-cache` through the Avrea GitHub
Actions cache proxy, plus sccache against the Avrea WebDAV endpoint
(`http://cache.avrea.com:8290/sccache-build/webdav`). `setup-rust` installs sccache,
sets `RUSTC_WRAPPER`, and starts the daemon. No sharding — a single Avrea Windows
runner (4 vCPU / 16 GB) with colocated caching covers the full pipeline.

Create these runner profiles in the [Namespace dashboard](https://cloud.namespace.so/workspace/actions/profiles) and enable a cache volume on each:

| `runs-on`                            | OS / arch     | Used by      |
| ------------------------------------ | ------------- | ------------ |
| `namespace-profile-linux-base-amd64` | Linux AMD64   | CI + release |
| `namespace-profile-macos-base-arm64` | macOS ARM64   | CI + release |
| `avrea-windows-2025-4-vcpu`          | Windows AMD64 | CI + release |

Connect the GitHub org to Namespace before the first run. Lightweight jobs (version gate, publish, sync) stay on GitHub `ubuntu-slim`.

`.cargo/config.toml` uses zig as the linker for musl targets (`x86_64-unknown-linux-musl`, `aarch64-unknown-linux-musl`). Linux AMD64 jobs download zig from ziglang.org in `setup-rust` for musl builds; the default glibc target uses the system linker.

## Test (debug)

Workflow: `.github/workflows/test.yml` (`Test / linux`, `Test / macos`, `Test / windows`).

Triggers:

- pull request
- push to `main`

Both use path filters on the elph workspace, lockfile, toolchain, Makefile, and `.github/`.

On Linux and macOS the job runs, in order: `cargo fmt --check`, `make check`, `make lint`, `make lint/test -p elph-agent --features full`, `make test`, then `make build`. Windows (`avrea-windows-2025-4-vcpu`, 90 min) runs fmt + check + lint + `make test` (and `make build` when the workflow mode is `full`). Extensions use the wasmi interpreter. Workflow concurrency cancels in-progress runs on pull requests only, not on `main`. Shell exec locates Git Bash (`where.exe` / `Git\\bin\\bash.exe`); abort/timeout uses `taskkill /T`. Auth store locking uses a sibling `.flock` file (NTFS mandatory locks cannot sit on `auth.json`). Home directories fall back to `USERPROFILE` when `HOME` is unset. With `CI=true` (or profiling flags like `make build -- --ci`), those targets use Cargo profile `ci` (`target/ci/`: `opt-level=0`, no debuginfo, no incremental — sccache is the cache). Local `make` stays on `dev`. Profiles: `--debug`, `--release`, `--dist`, `--ci` (last flag wins). `PROFILE=dist` / `--dist` is unchanged (`opt-level=3`, thin LTO, `codegen-units=1`).

## Release

Workflow: `.github/workflows/release.yml` (`Release / auth`, `version`, `check`, `linux`, `macos`, `windows`, `publish`, `sync`).

Trigger: push a tag matching `v*.*.*` (release, e.g. `v0.1.0`) or `v*.*.*-canary` (canary, e.g. `v0.1.0-canary`).

The channel is derived from the tag suffix:

- `v*.*.*` → **release** channel, built with the `dist` profile; published as a GitHub **pre-release** (never marked Latest).
- `v*.*.*-canary` → **canary** channel, built with the `release` profile; published as a GitHub **pre-release**.

Sequence:

1. Actor allow-list (`RELEASE_ALLOWED_ACTORS` repo variable, checked by `scripts/ci-check-release-actor.sh`).
2. Version gate (`.github/version-gate` → `scripts/ci-check-release-version.sh`) — tag version must be newer than the latest GitHub release _of the same channel_ and at least the version in `crates/coding-agent/Cargo.toml`.
3. Quality gate — fmt, check, lint only (no test, no debug build).
4. Binaries on Linux, macOS, and Windows — each built with `cargo build --profile dist` (release) or `--profile release` (canary), via the `BUILD_PROFILE` env derived from the tag.
5. GitHub pre-release (every tag) with archives and `SHA256SUMS`.
6. Sync `crates/coding-agent/Cargo.toml` on `main` if the tag version is ahead — **release channel only**; canary tags do not advance `main`.

App name `elph` maps to `crates/coding-agent/Cargo.toml` via `scripts/ci-app-manifest.sh`.
