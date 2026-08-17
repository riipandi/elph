#!/usr/bin/env bash
# Print the Cargo.toml path for a CI app name (elph → crates/coding-agent).
set -euo pipefail

app="${1:?app name is required (elph)}"
case "$app" in
elph) echo "crates/coding-agent/Cargo.toml" ;;
*) echo "${app}/Cargo.toml" ;;
esac
