# Known Issues

## 2026-07-04

- **Pi OS 32-bit (armv7)** — cross-compile fails; `turso`/`io-uring` does not support armv7. Use Pi OS **64-bit** → `*-linux-glibc-arm64.tar.gz`.
- **macOS** — no `cross-rs` Docker image; `*-macos-*` archives are produced only when `make cross` runs on a Mac.

Platform details: [docs/limitation.md](./docs/limitation.md).
