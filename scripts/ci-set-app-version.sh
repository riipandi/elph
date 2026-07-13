#!/usr/bin/env bash
# Set {app}/Cargo.toml and Cargo.lock to an exact semver.
set -euo pipefail

app="${APP:?APP is required (elph)}"
target="${TARGET_VERSION:?TARGET_VERSION is required}"
manifest="${app}/Cargo.toml"

if [[ ! -f "$manifest" ]]; then
    echo "Manifest not found: $manifest" >&2
    exit 1
fi

python3 -c '
import sys

parts = sys.argv[1].split(".")
if len(parts) != 3 or not all(part.isdigit() for part in parts):
    raise SystemExit(f"invalid semver: {sys.argv[1]}")
' "$target"

current="$(grep '^version = ' "$manifest" | head -1 | sed 's/.*= *"\(.*\)"/\1/')"
pkg_name="$(grep '^name = ' "$manifest" | head -1 | sed 's/.*= *"\(.*\)"/\1/')"

if [[ "$current" == "$target" ]]; then
    echo "${app} already at ${target}"
    exit 0
fi

if [[ "$(uname -s)" == "Darwin" ]]; then
    sed -i '' "s/^version = \"[^\"]*\"/version = \"${target}\"/" "$manifest"
else
    sed -i "s/^version = \"[^\"]*\"/version = \"${target}\"/" "$manifest"
fi

python3 -c '
import re
import sys
from pathlib import Path

pkg_name, target_version = sys.argv[1], sys.argv[2]
lock_path = Path("Cargo.lock")
lock = lock_path.read_text()
pattern = rf"(name = \"{re.escape(pkg_name)}\"\nversion = \")[^\"]+(\")"
new_lock, count = re.subn(pattern, rf"\g<1>{target_version}\2", lock, count=1)
if count != 1:
    raise SystemExit(f"failed to update Cargo.lock entry for {pkg_name}")
lock_path.write_text(new_lock)
' "$pkg_name" "$target"

echo "Set ${app} version: ${current} → ${target}"
