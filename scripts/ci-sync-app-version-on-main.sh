#!/usr/bin/env bash
# Sync {app}/Cargo.toml on main to the version that was just released.
set -euo pipefail

app="${APP:?APP is required (elph, eclaw, or owly)}"
released="${RELEASED_VERSION:?RELEASED_VERSION is required}"
manifest="${app}/Cargo.toml"

if [[ ! -f "$manifest" ]]; then
    echo "Manifest not found: $manifest" >&2
    exit 1
fi

current="$(grep '^version = ' "$manifest" | head -1 | sed 's/.*= *"\(.*\)"/\1/')"

already_synced="$(python3 -c '
import sys

def parse(version: str) -> tuple[int, int, int]:
    return tuple(int(part) for part in version.split("."))

print("true" if parse(sys.argv[1]) >= parse(sys.argv[2]) else "false")
' "$current" "$released")"

if [[ "$already_synced" == "true" ]]; then
    echo "${app}/Cargo.toml is already at ${current} (>= released ${released}); nothing to do"
    exit 0
fi

APP="$app" TARGET_VERSION="$released" "$(dirname "$0")/ci-set-app-version.sh"
echo "Synced ${app} on main to released version ${released} (was ${current})"
