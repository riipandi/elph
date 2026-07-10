#!/usr/bin/env bash
# Allow only the repository owner or explicitly listed GitHub users to run releases.
set -euo pipefail

actor="${GITHUB_ACTOR:?GITHUB_ACTOR is required}"
owner="${GITHUB_REPOSITORY_OWNER:?GITHUB_REPOSITORY_OWNER is required}"

echo "Release triggered by: ${actor}"
echo "Repository owner: ${owner}"

if [[ "$actor" == "$owner" ]]; then
    echo "Authorized: actor is the repository owner"
    exit 0
fi

if [[ -n "${RELEASE_ALLOWED_ACTORS:-}" ]]; then
    while IFS= read -r user; do
        user="${user#"${user%%[![:space:]]*}"}"
        user="${user%"${user##*[![:space:]]}"}"
        if [[ -n "$user" && "$actor" == "$user" ]]; then
            echo "Authorized: actor is listed in RELEASE_ALLOWED_ACTORS"
            exit 0
        fi
    done < <(printf '%s' "${RELEASE_ALLOWED_ACTORS}" | tr ',' '\n')
fi

{
    echo "Release denied: ${actor} is not authorized to publish releases" >&2
    echo "Allowed actors: repository owner (${owner})" >&2
    if [[ -n "${RELEASE_ALLOWED_ACTORS:-}" ]]; then
        echo "Additional allowed actors (RELEASE_ALLOWED_ACTORS): ${RELEASE_ALLOWED_ACTORS}" >&2
    else
        echo "To allow more users, set the RELEASE_ALLOWED_ACTORS repository variable (comma-separated GitHub usernames)." >&2
    fi
} >&2
exit 1
