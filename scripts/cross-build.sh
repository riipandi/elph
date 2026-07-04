#!/usr/bin/env bash
# Build and package eclaw + elph for one Rust target.
set -euo pipefail

target="${1:?usage: cross-build.sh <target-triple>}"

root="$(cd "$(dirname "$0")/.." && pwd)"
# shellcheck source=scripts/cross-host.sh
source "${root}/scripts/cross-host.sh"

cross_bin="$(command -v cross || true)"
cargo_bin="$(command -v cargo || true)"
stage="${root}/scripts/cross-stage.sh"

tool="$(cross_tool_for "$target")"

if [[ "$tool" == "skip" ]]; then
  echo "Skipped ${target} (not available on this host)" >&2
  exit 0
fi

if [[ "$tool" == "cross" && -z "$cross_bin" ]]; then
  echo "cross is required for ${target}; run: make prepare" >&2
  exit 1
fi

if [[ -z "$cargo_bin" ]]; then
  echo "cargo is required" >&2
  exit 1
fi

builder="$cargo_bin"
if [[ "$tool" == "cross" ]]; then
  builder="$cross_bin"
fi

for pkg in eclaw elph; do
  "$builder" build --release -p "$pkg" --target "$target"
  "$stage" "$target" "$pkg"
done
