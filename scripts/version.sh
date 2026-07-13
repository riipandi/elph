#!/usr/bin/env bash
# Report Cargo.toml versions vs latest GitHub releases.
# Optional TAG= validates a planned release tag (same rules as CI release gate).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

if [[ -n "${APP:-}" ]]; then
    APPS=("$APP")
else
    APPS=(elph)
fi

repo="${GITHUB_REPOSITORY:-}"
if [[ -z "$repo" ]]; then
    repo="$(git remote get-url origin 2>/dev/null | sed -E 's#.*github.com[:/ ]([^/]+/[^/.]+)(\.git)?$#\1#' || true)"
fi
repo="${repo:-riipandi/elph}"

api_url="https://api.github.com/repos/${repo}/releases?per_page=100"

if [[ -n "${GITHUB_TOKEN:-}" ]]; then
    json="$(curl -fsSL \
        -H "Accept: application/vnd.github+json" \
        -H "Authorization: Bearer ${GITHUB_TOKEN}" \
        "${api_url}")"
else
    json="$(curl -fsSL -H "Accept: application/vnd.github+json" "${api_url}")"
fi

latest_for_app() {
    local app="$1"
    printf '%s' "${json}" | python3 -c '
import json
import re
import sys

app = sys.argv[1]
prefix = f"{app}-v"

def ver_key(tag: str) -> tuple[int, int, int]:
    match = re.search(r"-v(\d+)\.(\d+)\.(\d+)", tag)
    if not match:
        return (0, 0, 0)
    return tuple(int(part) for part in match.groups())

releases = json.load(sys.stdin)
tags: list[str] = []
for release in releases:
    tag_name = release.get("tag_name", "")
    if not tag_name.startswith(prefix):
        continue
    if release.get("prerelease", False):
        continue
    if release.get("draft", False):
        continue
    tags.append(tag_name)

if not tags:
    sys.exit(0)

tags.sort(key=ver_key, reverse=True)
print(tags[0])
' "${app}"
}

cargo_version() {
    local app="$1"
    grep '^version = ' "${app}/Cargo.toml" | head -1 | sed 's/.*= *"\(.*\)"/\1/'
}

next_patch() {
    python3 -c '
import sys

parts = sys.argv[1].split(".")
major, minor, patch = (int(part) for part in parts)
print(f"{major}.{minor}.{patch + 1}")
' "$1"
}

compare_gt() {
    python3 -c '
import sys

def parse(version: str) -> tuple[int, int, int]:
    return tuple(int(part) for part in version.split("."))

print("true" if parse(sys.argv[1]) > parse(sys.argv[2]) else "false")
' "$1" "$2"
}

compare_gte() {
    python3 -c '
import sys

def parse(version: str) -> tuple[int, int, int]:
    return tuple(int(part) for part in version.split("."))

print("true" if parse(sys.argv[1]) >= parse(sys.argv[2]) else "false")
' "$1" "$2"
}

echo "Repository: ${repo}"
printf '%-6s  %-14s  %-14s  %-18s  %s\n' "app" "cargo.toml" "latest release" "suggested tag" "status"
echo "──────  ──────────────  ──────────────  ──────────────────  ─────────────────────────────"

status=0
for app in "${APPS[@]}"; do
    if [[ ! -f "${app}/Cargo.toml" ]]; then
        echo "error: missing ${app}/Cargo.toml" >&2
        exit 1
    fi

    cargo="$(cargo_version "$app")"
    latest_tag="$(latest_for_app "$app" 2>/dev/null || true)"
    if [[ -z "${latest_tag}" ]]; then
        latest="(none)"
        suggested="${app}-v${cargo}"
        note="no stable release yet"
    else
        latest="${latest_tag#${app}-v}"
        suggested="${app}-v$(next_patch "$latest")"
        if [[ "$(compare_gt "$cargo" "$latest")" == "true" ]]; then
            note="main ahead of latest release"
        elif [[ "$cargo" == "$latest" ]]; then
            note="main synced with latest release"
        else
            note="main behind latest release"
            status=1
        fi
    fi

    printf '%-6s  %-14s  %-14s  %-18s  %s\n' "$app" "$cargo" "$latest" "$suggested" "$note"
done

if [[ -n "${TAG:-}" ]]; then
    echo ""
    tag_app="${TAG%%-v*}"
    if [[ "$tag_app" == "$TAG" || -z "$tag_app" ]]; then
        echo "error: invalid tag format (expected <app>-v<major>.<minor>.<patch>): ${TAG}" >&2
        exit 1
    fi

    case "$tag_app" in
    elph) ;;
    *)
        echo "error: unknown app in tag: ${tag_app}" >&2
        exit 1
        ;;
    esac

    tag_version="${TAG#${tag_app}-v}"
    cargo="$(cargo_version "$tag_app")"
    latest_tag="$(latest_for_app "$tag_app" 2>/dev/null || true)"
    latest="${latest_tag#${tag_app}-v}"
    if [[ -z "${latest_tag}" ]]; then
        latest="(none)"
    fi

    echo "Validate tag: ${TAG}"
    echo "  cargo.toml (${tag_app}): ${cargo}"
    echo "  latest release:          ${latest}"

    if [[ "$(compare_gte "$tag_version" "$cargo")" != "true" ]]; then
        echo "error: tag version ${tag_version} is older than ${tag_app}/Cargo.toml (${cargo})" >&2
        exit 1
    fi

    if [[ -n "${latest_tag}" && "$(compare_gt "$tag_version" "$latest")" != "true" ]]; then
        echo "error: tag version ${tag_version} is not newer than latest release ${latest}" >&2
        exit 1
    fi

    echo "ok: ${TAG} is valid for release"
fi

exit "$status"
