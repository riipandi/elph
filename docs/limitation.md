# Known Limitation

## Release (`make release`)

| Archive                       | Target                       | For                             |
|-------------------------------|------------------------------|---------------------------------|
| `*-linux-glibc-x86_64.tar.gz` | `x86_64-unknown-linux-gnu`   | RedHat/Ubuntu/Debian x86_64     |
| `*-linux-glibc-arm64.tar.gz`  | `aarch64-unknown-linux-gnu`  | Pi 3/4/5 64-bit OS, ARM64 glibc |
| `*-linux-musl-x86_64.tar.gz`  | `x86_64-unknown-linux-musl`  | Alpine x86_64                   |
| `*-linux-musl-arm64.tar.gz`   | `aarch64-unknown-linux-musl` | Alpine ARM64                    |
| `*-macos-*.tar.gz`            | `*-apple-darwin`             | macOS (Mac host only)           |
| `*-win-*.zip`                 | `*-pc-windows-*`             | Windows                         |

## Not supported

- **Pi OS 32-bit** (`armv7`) — `turso`/`io-uring` limitation
- **Android, iOS**, and other mobile/embedded targets

## Single platform

```sh
make cross CROSS_TARGET=aarch64-unknown-linux-musl
```
