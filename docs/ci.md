# CI / CD

GitHub Actions run on [Namespace](https://namespace.so/docs/solutions/github-actions) runners. Compiler artifacts go through [sccache](https://github.com/mozilla/sccache) backed by a Namespace cache (`nsc cache sccache setup`). Cargo registry and `target/` live on a Namespace cache volume (`namespacelabs/nscloud-cache-action`). Windows release builds run on GitHub-hosted `windows-latest` (Namespace has no Windows runner yet).

### sccache / Namespace

`setup-rust` provisions sccache credentials automatically on Namespace runners via
`nsc cache sccache setup --cache_name default` (WebDAV backend). No R2 secrets or
repository variables are required. The composite sets `RUSTC_WRAPPER=sccache`,
`SCCACHE_DIRECT=true`, and `SCCACHE_MAXSIZE=20G`. Windows builds use `cacheBackend: rust-cache`
(GitHub Actions cache) and do not invoke sccache.

Create these runner profiles in the [Namespace dashboard](https://cloud.namespace.so/workspace/actions/profiles) and enable a cache volume on each:

| `runs-on`                              | OS / arch     | Used by      |
| -------------------------------------- | ------------- | ------------ |
| `namespace-profile-linux-base-amd64`   | Linux AMD64   | CI + release |
| `namespace-profile-macos-base-arm64`   | macOS ARM64   | CI + release |
| `windows-latest` (GitHub-hosted)       | Windows AMD64 | Release only |

Connect the GitHub org to Namespace before the first run. Lightweight jobs (version gate, publish, sync) stay on GitHub `ubuntu-slim`.

`.cargo/config.toml` uses the [wild](https://github.com/wild-linker/wild) linker for `x86_64-unknown-linux-gnu` (`clang --ld-path=wild`). Linux AMD64 jobs install `clang` and `wild-linker` (`cargo binstall wild-linker`) before compiling.

## Test (debug)

Workflow: `.github/workflows/test.yml` (`Test / linux`, `Test / macos`, `Test / windows`).

Triggers:

- pull request
- push to `main`

Both use path filters on the elph workspace, lockfile, toolchain, Makefile, and `.github/`.

On Linux and macOS the job runs, in order: `cargo fmt --check`, `make check`, `make lint`, `make lint/test -p elph-agent --features full`, `make test`, then `make build`. Windows (`windows-latest`, 90 min) runs fmt + check + lint + `make test` only: compiling wasmtime (`elph-agent` `full`) and a second debug build routinely exceed the gap between `main` pushes and get `The operation was canceled`. Workflow concurrency cancels in-progress runs on pull requests only, not on `main`. Shell exec locates Git Bash (`where.exe` / `Git\\bin\\bash.exe`); abort/timeout uses `taskkill /T`. Auth store locking uses a sibling `.flock` file (NTFS mandatory locks cannot sit on `auth.json`). Home directories fall back to `USERPROFILE` when `HOME` is unset. With `CI=true` (or profiling flags like `make build -- --ci`), those targets use Cargo profile `ci` (`target/ci/`: `opt-level=0`, no debuginfo, no incremental — sccache is the cache). Local `make` stays on `dev`. Profiles: `--debug`, `--release`, `--dist`, `--ci` (last flag wins). `PROFILE=dist` / `--dist` is unchanged (`opt-level=3`, thin LTO, `codegen-units=1`).

## Release

Workflow: `.github/workflows/release.yml` (`Release / auth`, `version`, `check`, `linux`, `macos`, `windows`, `publish`, `sync`).

Trigger: push a tag matching `v*.*.*` (release, e.g. `v0.1.0`) or `v*.*.*-canary` (canary, e.g. `v0.1.0-canary`).

The channel is derived from the tag suffix:

- `v*.*.*` → **release** channel, built with the `dist` profile; published as a GitHub **pre-release** (never marked Latest).
- `v*.*.*-canary` → **canary** channel, built with the `release` profile; published as a GitHub **pre-release**.

Sequence:

1. Actor allow-list (`RELEASE_ALLOWED_ACTORS` repo variable, checked by `scripts/ci-check-release-actor.sh`).
2. Version gate (`.github/version-gate` → `scripts/ci-check-release-version.sh`) — tag version must be newer than the latest GitHub release *of the same channel* and at least the version in `crates/coding-agent/Cargo.toml`.
3. Quality gate — fmt, check, lint only (no test, no debug build).
4. Binaries on Linux, macOS, and Windows — each built with `cargo build --profile dist` (release) or `--profile release` (canary), via the `BUILD_PROFILE` env derived from the tag.
5. GitHub pre-release (every tag) with archives and `SHA256SUMS`.
6. Sync `crates/coding-agent/Cargo.toml` on `main` if the tag version is ahead — **release channel only**; canary tags do not advance `main`.

App name `elph` maps to `crates/coding-agent/Cargo.toml` via `scripts/ci-app-manifest.sh`.
