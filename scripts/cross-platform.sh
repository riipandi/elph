#!/usr/bin/env bash
# Build release bundles for one platform family (host-aware).
set -euo pipefail

platform="${1:?usage: cross-platform.sh <linux-glibc|linux-musl|macos|windows>}"

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

# shellcheck source=scripts/cross-host.sh
source "${root}/scripts/cross-host.sh"

case "$platform" in
linux-glibc)
  targets=(
    x86_64-unknown-linux-gnu
    aarch64-unknown-linux-gnu
  )
  ;;
linux-musl)
  targets=(
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
  echo "usage: cross-platform.sh <linux-glibc|linux-musl|macos|windows>" >&2
  exit 1
  ;;
esac

cross_print_plan "${targets[@]}"

for target in "${targets[@]}"; do
  "${root}/scripts/cross-build.sh" "$target"
done

echo "release/"
ls -1 "${root}/release" 2>/dev/null | sort || true
