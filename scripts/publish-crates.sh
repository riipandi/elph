#!/usr/bin/env bash
# Publish workspace crates to crates.io in dependency order.

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

CARGO="${CARGO:-cargo}"
ALLOW_DIRTY="${ALLOW_DIRTY:---allow-dirty}"
DRY_RUN="${DRY_RUN:-0}"

# elph-core is published first (no internal workspace deps).
PACKAGES=(
    elph-core
    elph-ai
    elph-agent
    elph-swarm
    elph-tui
    elph
    eclaw
    owly
)

pkg_manifest() {
    local pkg=$1
    case "$pkg" in
    elph) echo "elph/Cargo.toml" ;;
    eclaw) echo "eclaw/Cargo.toml" ;;
    owly) echo "owly/Cargo.toml" ;;
    *) echo "crates/${pkg}/Cargo.toml" ;;
    esac
}

pkg_version() {
    local pkg=$1
    grep '^version = ' "$(pkg_manifest "$pkg")" | head -1 | sed 's/.*= *"\(.*\)"/\1/'
}

publish_dry_run() {
    local pkg=$1
    $CARGO publish -p "$pkg" --dry-run $ALLOW_DIRTY 2>&1
}

# Full dry-run when deps are on crates.io; otherwise validate the crate still builds.
publishable() {
    local pkg=$1
    local err

    if publish_dry_run "$pkg" >/dev/null 2>&1; then
        return 0
    fi

    if [[ "$DRY_RUN" != "1" ]]; then
        return 1
    fi

    err="$(publish_dry_run "$pkg" 2>&1 || true)"
    if echo "$err" | grep -qE 'no matching package named|failed to select a version for the requirement'; then
        $CARGO check -p "$pkg" --quiet >/dev/null 2>&1
        return 0
    fi

    return 1
}

publish_pkg() {
    local pkg=$1
    local ver
    ver="$(pkg_version "$pkg")"

    if ! publishable "$pkg"; then
        echo "error: $pkg v$ver failed publish dry-run" >&2
        publish_dry_run "$pkg" 2>&1 | tail -8 >&2 || true
        return 1
    fi

    if [[ "$DRY_RUN" == "1" ]]; then
        if publish_dry_run "$pkg" >/dev/null 2>&1; then
            echo "Would publish $pkg v$ver"
        else
            echo "Would publish $pkg v$ver (deps not yet on crates.io; cargo check ok)"
        fi
        return 0
    fi

    echo "Publishing $pkg v$ver to crates.io"
    $CARGO publish --quiet -p "$pkg" $ALLOW_DIRTY
    return 0
}

published=0
failed=0

echo "==> Publishing workspace crates"
for pkg in "${PACKAGES[@]}"; do
    if publish_pkg "$pkg"; then
        published=$((published + 1))
    else
        failed=$((failed + 1))
        break
    fi
done

echo ""
echo "Summary: published=$published failed=$failed"
if [[ "$failed" -gt 0 ]]; then
    exit 1
fi
