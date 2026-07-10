#!/usr/bin/env bash
# Compare {app}/Cargo.toml version against the latest stable GitHub release for that app.
# Writes should_continue=true|false to GITHUB_OUTPUT.
# Continue only when Cargo.toml version is strictly greater than the latest release.
set -euo pipefail

app="${APP:?APP is required (elph, eclaw, or owly)}"
manifest="${app}/Cargo.toml"
output="${GITHUB_OUTPUT:?GITHUB_OUTPUT is required}"

if [[ ! -f "$manifest" ]]; then
    echo "Manifest not found: $manifest" >&2
    exit 1
fi

cargo_version="$(grep '^version = ' "$manifest" | head -1 | sed 's/.*= *"\(.*\)"/\1/')"
prefix="${app}-v"
repo="${GITHUB_REPOSITORY:-riipandi/elph}"
api_url="https://api.github.com/repos/${repo}/releases?per_page=100"

echo "App: ${app}"
echo "Cargo.toml version: ${cargo_version}"

if [[ -n "${GITHUB_TOKEN:-}" ]]; then
    json="$(curl -fsSL \
        -H "Accept: application/vnd.github+json" \
        -H "Authorization: Bearer ${GITHUB_TOKEN}" \
        "${api_url}")"
else
    json="$(curl -fsSL -H "Accept: application/vnd.github+json" "${api_url}")"
fi

latest_tag="$(printf '%s' "${json}" | python3 -c '
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
' "${app}")" || latest_tag=""

if [[ -z "${latest_tag}" ]]; then
    echo "No stable ${prefix}* releases found; continuing pipeline"
    {
        echo "should_continue=true"
        echo "cargo_version=${cargo_version}"
        echo "latest_release=none"
    } >>"${output}"
    exit 0
fi

latest_version="${latest_tag#${prefix}}"
echo "Latest GitHub release: ${latest_tag}"

should_continue="$(python3 -c '
import sys

def parse(version: str) -> tuple[int, int, int]:
    return tuple(int(part) for part in version.split("."))

cargo = parse(sys.argv[1])
latest = parse(sys.argv[2])
print("true" if cargo > latest else "false")
' "${cargo_version}" "${latest_version}")"

if [[ "${should_continue}" == "true" ]]; then
    echo "Cargo.toml version ${cargo_version} is newer than ${latest_version}; continuing pipeline"
else
    echo "Cargo.toml version ${cargo_version} is not newer than latest release ${latest_version}; skipping remaining jobs"
fi

{
    echo "should_continue=${should_continue}"
    echo "cargo_version=${cargo_version}"
    echo "latest_release=${latest_version}"
} >>"${output}"
