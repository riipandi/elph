#!/usr/bin/env bash
# Build the full release/ bundle for elph (host-aware).
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

# shellcheck source=scripts/cross-host.sh
source "${root}/scripts/cross-host.sh"

cross_bin="$(command -v cross || true)"
cargo_bin="$(command -v cargo || true)"

if [[ -z "$cross_bin" || -z "$cargo_bin" ]]; then
    echo "cross and cargo are required; run: make prepare" >&2
    exit 1
fi

# armv7 excluded — turso/io-uring does not support 32-bit ARM (see KNOWN_ISSUES.md)
all_targets=(
    x86_64-unknown-linux-gnu
    aarch64-unknown-linux-gnu
    x86_64-unknown-linux-musl
    aarch64-unknown-linux-musl
    x86_64-pc-windows-gnu
    aarch64-pc-windows-msvc
    x86_64-apple-darwin
    aarch64-apple-darwin
)

_start=$(python3 -c "import time; print(int(time.time()*1000))")

cross_log_banner "Cross-release  all platforms"
echo
cross_print_plan "${all_targets[@]}"

for target in "${all_targets[@]}"; do
    "${root}/scripts/cross-build.sh" "$target"
    echo
done

cross_print_release_tree "$root"
echo

_end=$(python3 -c "import time; print(int(time.time()*1000))")
_elapsed=$((_end - _start))
printf 'Done in %d.%03ds\n' $((_elapsed / 1000)) $((_elapsed % 1000))
