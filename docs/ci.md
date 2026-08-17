# CI / CD

GitHub Actions run on [Namespace](https://namespace.so/docs/solutions/github-actions) runners. Compiler artifacts go through [sccache](https://github.com/mozilla/sccache) against a Cloudflare R2 bucket (S3-compatible). Cargo registry and `target/` live on a Namespace cache volume (`namespacelabs/nscloud-cache-action`).

### sccache / R2

Set these on the GitHub repository (Settings → Secrets and variables → Actions). They match the local `AWS_PROFILE=r2-sccache` bucket.

| Kind   | Name                          | Value |
| ------ | ----------------------------- | ----- |
| Secret | `SCCACHE_R2_ACCESS_KEY_ID`    | R2 API token access key |
| Secret | `SCCACHE_R2_SECRET_ACCESS_KEY`| R2 API token secret |
| Variable | `SCCACHE_R2_BUCKET`         | Bucket name |
| Variable | `SCCACHE_R2_ENDPOINT`       | `https://<accountid>.r2.cloudflarestorage.com` |
| Variable | `SCCACHE_R2_REGION`         | Optional; defaults to `auto` |

CI maps those to `AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY`, `SCCACHE_BUCKET`, `SCCACHE_ENDPOINT`, and `SCCACHE_REGION`. Also set: `SCCACHE_S3_USE_SSL=true`, `SCCACHE_DIRECT=true`, `SCCACHE_MAXSIZE=50G`. If `SCCACHE_R2_BUCKET` is empty, sccache still wraps rustc but has no remote cache.

Create these runner profiles in the [Namespace dashboard](https://cloud.namespace.so/workspace/actions/profiles) and enable a cache volume on each:

| `runs-on`                                 | OS / arch        | Used by        |
| ----------------------------------------- | ---------------- | -------------- |
| `namespace-profile-linux-base-amd64`      | Linux AMD64      | CI + release   |
| `namespace-profile-macos-base-arm64`      | macOS ARM64      | CI + release   |
| `namespace-profile-windows-base-amd64`    | Windows AMD64    | Release only   |

Connect the GitHub org to Namespace before the first run. Lightweight jobs (version gate, publish, sync) stay on GitHub `ubuntu-slim`.

`.cargo/config.toml` uses the [wild](https://github.com/wild-linker/wild) linker for `x86_64-unknown-linux-gnu` (`clang --ld-path=wild`). Linux AMD64 jobs install `clang` and `wild-linker` (`cargo binstall wild-linker`) before compiling.

## Test (debug)

Workflow: `.github/workflows/test.yml` (`Test / linux`, `Test / macos`).

Triggers:

- pull request
- push to `main`

Both use path filters on the elph workspace, lockfile, toolchain, Makefile, and `.github/`.

On Linux and macOS the job runs, in order: `cargo fmt --check`, `make check-elph`, `make lint-elph`, `make test-elph`, then `make build-elph` (debug profile — no `RELEASE=1`). Release-profile compilation is not part of this workflow.

## Release

Workflow: `.github/workflows/release.yml` (`Release / auth`, `version`, `check`, `linux`, `macos`, `windows`, `publish`, `sync`).

Trigger: push a tag matching `elph-v*.*.*` (for example `elph-v0.1.0`).

Sequence:

1. Actor allow-list (`RELEASE_ALLOWED_ACTORS` repo variable, checked by `scripts/ci-check-release-actor.sh`).
2. Version gate (`.github/version-gate` → `scripts/ci-check-release-version.sh`) — tag version must be newer than the latest GitHub release and at least the version in `crates/coding-agent/Cargo.toml`.
3. Quality gate — fmt, check, lint only (no test, no debug build).
4. Release-profile binaries on Linux, macOS (`make release-macos`), and Windows.
5. GitHub Release with archives and `SHA256SUMS`.
6. Sync `crates/coding-agent/Cargo.toml` on `main` if the tag version is ahead.

App name `elph` maps to `crates/coding-agent/Cargo.toml` via `scripts/ci-app-manifest.sh`.
