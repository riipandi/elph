#!/usr/bin/env bash
# Print rustup targets used for cross-compilation.
set -euo pipefail

targets=(
  x86_64-unknown-linux-gnu
  aarch64-unknown-linux-gnu
  x86_64-unknown-linux-musl
  aarch64-unknown-linux-musl
  x86_64-pc-windows-gnu
  aarch64-pc-windows-msvc
  x86_64-apple-darwin
  aarch64-apple-darwin
)

printf '%s\n' "${targets[@]}"
