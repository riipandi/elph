#!/usr/bin/env bash
# Release version gate: the pushed tag is the source of truth.
# Continue when tag version is strictly greater than the latest stable release
# and greater than or equal to the version in {app}/Cargo.toml on the tag commit.
set -euo pipefail

app="${APP:?APP is required (elph)}"
tag="${GITHUB_REF_NAME:?GITHUB_REF_NAME is required (e.g. elph-v0.0.28)}"
manifest="${app}/Cargo.toml"
output="${GITHUB_OUTPUT:?GITHUB_OUTPUT is required}"
prefix="${app}-v"

if [[ ! -f "$manifest" ]]; then
    echo "Manifest not found: $manifest" >&2
    exit 1
fi

if [[ "$tag" != "${prefix}"* ]]; then
    echo "Tag ${tag} does not match expected prefix ${prefix}" >&2
    exit 1
fi

tag_version="${tag#"${prefix}"}"
cargo_version="$(grep '^version = ' "$manifest" | head -1 | sed 's/.*= *"\(.*\)"/\1/')"
repo="${GITHUB_REPOSITORY:-riipandi/elph}"
api_url="https://api.github.com/repos/${repo}/releases?per_page=100"

echo "App: ${app}"
echo "Tag version: ${tag_version}"
echo "Cargo.toml version on tag commit: ${cargo_version}"

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

manifest_ok="$(python3 -c '
import sys

def parse(version: str) -> tuple[int, int, int]:
    return tuple(int(part) for part in version.split("."))

tag = parse(sys.argv[1])
cargo = parse(sys.argv[2])
print("true" if tag >= cargo else "false")
' "${tag_version}" "${cargo_version}")"

if [[ "${manifest_ok}" != "true" ]]; then
    echo "Tag version ${tag_version} is older than ${app}/Cargo.toml (${cargo_version})" >&2
    {
        echo "should_continue=false"
        echo "release_version=${tag_version}"
        echo "cargo_version=${cargo_version}"
        echo "latest_release=unknown"
    } >>"${output}"
    exit 0
fi

if [[ -z "${latest_tag}" ]]; then
    echo "No stable ${prefix}* releases found; continuing pipeline"
    {
        echo "should_continue=true"
        echo "release_version=${tag_version}"
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

tag = parse(sys.argv[1])
latest = parse(sys.argv[2])
print("true" if tag > latest else "false")
' "${tag_version}" "${latest_version}")"

if [[ "${should_continue}" == "true" ]]; then
    echo "Tag version ${tag_version} is newer than latest release ${latest_version}; continuing pipeline"
else
    echo "Tag version ${tag_version} is not newer than latest release ${latest_version}; skipping remaining jobs"
fi

{
    echo "should_continue=${should_continue}"
    echo "release_version=${tag_version}"
    echo "cargo_version=${cargo_version}"
    echo "latest_release=${latest_version}"
} >>"${output}"
