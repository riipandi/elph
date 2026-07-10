# Platform Limitations

Known release and platform constraints.

## Release archives (`make release`)

| Archive                       | Target                       | For                             |
| ----------------------------- | ---------------------------- | ------------------------------- |
| `*-linux-glibc-x86_64.tar.gz` | `x86_64-unknown-linux-gnu`   | RedHat/Ubuntu/Debian x86_64     |
| `*-linux-glibc-arm64.tar.gz`  | `aarch64-unknown-linux-gnu`  | Pi 3/4/5 64-bit OS, ARM64 glibc |
| `*-linux-musl-x86_64.tar.gz`  | `x86_64-unknown-linux-musl`  | Alpine x86_64                   |
| `*-linux-musl-arm64.tar.gz`   | `aarch64-unknown-linux-musl` | Alpine ARM64                    |
| `*-macos-*.tar.gz`            | `*-apple-darwin`             | macOS (built on Mac host)       |
| `*-win-*.zip`                 | `*-pc-windows-*`             | Windows                         |

## Not supported

- **Pi OS 32-bit** (`armv7`) — Turso / io-uring constraint
- **Android, iOS**, and other mobile/embedded targets

## Single-platform cross build

Use `make cross CROSS_TARGET=<triple>` (e.g. `aarch64-unknown-linux-musl`) to build one target at a time.
