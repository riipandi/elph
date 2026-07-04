#!/usr/bin/env bash
# Package a cross-compiled binary into release/<bin>-<platform>-<arch>.{tar.gz,zip}
# Linux: linux-glibc-* (glibc) and linux-musl-* (musl/Alpine)
# and refresh release/SHA256SUMS.
set -euo pipefail

target="${1:?usage: cross-stage.sh <target-triple> <binary-name>}"
bin="${2:?usage: cross-stage.sh <target-triple> <binary-name>}"

root="$(cd "$(dirname "$0")/.." && pwd)"
bundle_dir="${root}/release"
mkdir -p "$bundle_dir"

platform=""
arch=""
pack="tar.gz"

case "$target" in
x86_64-unknown-linux-gnu)
  platform="linux-glibc"
  arch="x86_64"
  ;;
aarch64-unknown-linux-gnu)
  platform="linux-glibc"
  arch="arm64"
  ;;
x86_64-unknown-linux-musl)
  platform="linux-musl"
  arch="x86_64"
  ;;
aarch64-unknown-linux-musl)
  platform="linux-musl"
  arch="arm64"
  ;;
x86_64-apple-darwin)
  platform="macos"
  arch="x86_64"
  ;;
aarch64-apple-darwin)
  platform="macos"
  arch="arm64"
  ;;
x86_64-pc-windows-gnu | x86_64-pc-windows-msvc)
  platform="win"
  arch="x86_64"
  pack="zip"
  ;;
aarch64-pc-windows-msvc)
  platform="win"
  arch="arm64"
  pack="zip"
  ;;
*)
  echo "unsupported release target: $target" >&2
  exit 1
  ;;
esac

src="${root}/target/${target}/release/${bin}"
artifact_name="${bin}-${platform}-${arch}.${pack}"
artifact_path="${bundle_dir}/${artifact_name}"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

if [[ "$pack" == "zip" ]]; then
  src="${src}.exe"
  if [[ ! -f "$src" ]]; then
    echo "binary not found: $src" >&2
    exit 1
  fi
  cp "$src" "${tmp}/${bin}.exe"
  (cd "$tmp" && zip -q -j "$artifact_path" "${bin}.exe")
else
  if [[ ! -f "$src" ]]; then
    echo "binary not found: $src" >&2
    exit 1
  fi
  cp "$src" "${tmp}/${bin}"
  chmod +x "${tmp}/${bin}"
  tar -C "$tmp" -czf "$artifact_path" "$bin"
fi

(
  cd "$bundle_dir"
  rm -f SHA256SUMS
  shopt -s nullglob
  artifacts=(*.tar.gz *.zip)
  if ((${#artifacts[@]} == 0)); then
    echo "no release archives to checksum in ${bundle_dir}" >&2
    exit 1
  fi
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum -- "${artifacts[@]}" >SHA256SUMS
  else
    shasum -a 256 -- "${artifacts[@]}" >SHA256SUMS
  fi
)

if stat -f%z "$artifact_path" >/dev/null 2>&1; then
  bytes=$(stat -f%z "$artifact_path")
else
  bytes=$(stat -c%s "$artifact_path")
fi
size_mb=$((bytes / 1048576))
checksum=$(grep " ${artifact_name}\$" "${bundle_dir}/SHA256SUMS" | awk '{print $1}')

printf 'Packaged ./release/%s %dMB %s\n' "$artifact_name" "$size_mb" "$checksum"
