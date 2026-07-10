#!/usr/bin/env bash
# Print rustup targets used for cross-compilation.
# Optional platform filter: linux | macos | windows | all (default).
set -euo pipefail

platform="${1:-all}"

case "$platform" in
all)
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
    ;;
linux)
    targets=(
        x86_64-unknown-linux-gnu
        aarch64-unknown-linux-gnu
        x86_64-unknown-linux-musl
        aarch64-unknown-linux-musl
    )
    ;;
macos)
    targets=(
        x86_64-apple-darwin
        aarch64-apple-darwin
    )
    ;;
windows)
    targets=(
        x86_64-pc-windows-gnu
        aarch64-pc-windows-msvc
    )
    ;;
*)
    echo "unknown platform: $platform" >&2
    echo "usage: cross-targets.sh [all|linux|macos|windows]" >&2
    exit 1
    ;;
esac

printf '%s\n' "${targets[@]}"
