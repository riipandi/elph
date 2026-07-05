#!/usr/bin/env bash
# Stage cross-compiled binaries and archives under release/.
#   release/binaries/<bin>-<platform>-<arch>[.exe]
#   release/archives/<bin>-<platform>-<arch>.{tar.gz,zip}
# Linux: linux-glibc-* (glibc) and linux-musl-* (musl/Alpine)
set -euo pipefail

target="${1:?usage: cross-stage.sh <target-triple> <binary-name>}"
bin="${2:?usage: cross-stage.sh <target-triple> <binary-name>}"

root="$(cd "$(dirname "$0")/.." && pwd)"
# shellcheck source=scripts/cross-host.sh
source "${root}/scripts/cross-host.sh"

archives_dir="${root}/release/archives"
binaries_dir="${root}/release/binaries"
mkdir -p "$archives_dir" "$binaries_dir"

platform=""
arch=""
pack="tar.gz"
is_windows=0

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
    is_windows=1
    ;;
aarch64-pc-windows-msvc)
    platform="win"
    arch="arm64"
    pack="zip"
    is_windows=1
    ;;
*)
    echo "unsupported release target: $target" >&2
    exit 1
    ;;
esac

refresh_checksums() {
    local dir="$1"
    shift
    (
        cd "$dir"
        rm -f SHA256SUMS
        shopt -s nullglob
        local -a files=()
        if ((${#@} > 0)); then
            local pattern
            for pattern in "$@"; do
                files+=($pattern)
            done
        else
            local entry
            for entry in *; do
                [[ "$entry" == "SHA256SUMS" || ! -f "$entry" ]] && continue
                files+=("$entry")
            done
        fi
        if ((${#files[@]} == 0)); then
            echo "no files to checksum in ${dir}" >&2
            exit 1
        fi
        if command -v rapidhash >/dev/null 2>&1; then
            for f in "${files[@]}"; do
                printf '%s  %s\n' "$(rapidhash "$f")" "$f"
            done >SHA256SUMS
        elif command -v sha256sum >/dev/null 2>&1; then
            sha256sum -- "${files[@]}" >SHA256SUMS
        else
            shasum -a 256 -- "${files[@]}" >SHA256SUMS
        fi
    )
}

base_name="${bin}-${platform}-${arch}"
src="${root}/target/${target}/release/${bin}"
binary_name="$base_name"
artifact_name="${base_name}.${pack}"
binary_path="${binaries_dir}/${binary_name}"
artifact_path="${archives_dir}/${artifact_name}"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

if [[ "$is_windows" == 1 ]]; then
    src="${src}.exe"
    binary_name="${base_name}.exe"
    binary_path="${binaries_dir}/${binary_name}"
fi

if [[ ! -f "$src" ]]; then
    echo "binary not found: $src" >&2
    exit 1
fi

cp "$src" "$binary_path"
chmod +x "$binary_path"

if [[ "$pack" == "zip" ]]; then
    cp "$src" "${tmp}/${bin}.exe"
    (cd "$tmp" && zip -q -j "$artifact_path" "${bin}.exe")
else
    cp "$src" "${tmp}/${bin}"
    chmod +x "${tmp}/${bin}"
    tar -C "$tmp" -czf "$artifact_path" "$bin"
fi

refresh_checksums "$binaries_dir" eclaw-* elph-*
refresh_checksums "$archives_dir" '*.tar.gz' '*.zip'

file_bytes() {
    if stat -f%z "$1" >/dev/null 2>&1; then
        stat -f%z "$1"
    else
        stat -c%s "$1"
    fi
}

binary_bytes=$(file_bytes "$binary_path")
archive_bytes=$(file_bytes "$artifact_path")
checksum=$(grep " ${artifact_name}\$" "${archives_dir}/SHA256SUMS" | awk '{print $1}')

cross_log_artifact "$bin" "$binary_name" "$artifact_name" "$binary_bytes" "$archive_bytes" "$checksum"
