#!/usr/bin/env bash
# Pre-pull ghcr.io/cross-rs images into local Docker cache.
# cross reuses these automatically on subsequent builds.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
# shellcheck source=scripts/cross-host.sh
source "${root}/scripts/cross-host.sh"

if ! command -v docker >/dev/null; then
  echo "docker is required" >&2
  exit 1
fi

cross_ver=""
if command -v cross >/dev/null; then
  cross_ver="$(cross --version 2>/dev/null | awk 'NR==1 {print $2}')"
fi
tag="${CROSS_IMAGE_TAG:-${cross_ver:-main}}"

# cross-rs images are linux/amd64 only; Apple Silicon / ARM64 hosts need --platform.
docker_platform=()
if [[ -n "${CROSS_DOCKER_PLATFORM:-}" ]]; then
  docker_platform=(--platform "$CROSS_DOCKER_PLATFORM")
elif [[ "$(cross_host_rust_arch)" == "aarch64" ]]; then
  docker_platform=(--platform linux/amd64)
fi

# Docker targets only (macOS has no cross-rs image)
all_targets=(
  x86_64-unknown-linux-gnu
  aarch64-unknown-linux-gnu
  x86_64-unknown-linux-musl
  aarch64-unknown-linux-musl
  x86_64-pc-windows-gnu
  aarch64-pc-windows-msvc
)

# ghcr.io/cross-rs/<target> is not published for every Rust triple (e.g. aarch64-pc-windows-msvc).
cross_image_published() {
  case "$1" in
  aarch64-pc-windows-msvc) return 1 ;;
  *) return 0 ;;
  esac
}

docker_targets=()
for target in "${all_targets[@]}"; do
  if [[ "$(cross_tool_for "$target")" == "cross" ]] && cross_image_published "$target"; then
    docker_targets+=("$target")
  fi
done

skipped_targets=()
for target in "${all_targets[@]}"; do
  if [[ "$(cross_tool_for "$target")" == "cross" ]] && ! cross_image_published "$target"; then
    skipped_targets+=("$target")
  fi
done

echo "Host: $(cross_host_label)"
if ((${#docker_platform[@]} > 0)); then
  echo "Docker platform: ${docker_platform[1]}"
fi
echo "Pulling cross-rs images (tag: ${tag})"
if ((${#skipped_targets[@]} > 0)); then
  echo "Skipping (no ghcr.io image): ${skipped_targets[*]}"
fi
echo

if ((${#docker_targets[@]} == 0)); then
  echo "No cross-rs images needed on this host."
  exit 0
fi

pull_image() {
  local target="$1"
  local image="ghcr.io/cross-rs/${target}:${tag}"
  echo "=> ${image}"
  if docker pull "${docker_platform[@]}" "$image"; then
    return 0
  fi
  if [[ "$tag" != "main" ]]; then
    local fallback="ghcr.io/cross-rs/${target}:main"
    echo "   retry ${fallback}"
    docker pull "${docker_platform[@]}" "$fallback"
  else
    return 1
  fi
}

for target in "${docker_targets[@]}"; do
  pull_image "$target" || exit 1
done

echo
echo "Cached images:"
docker image ls 'ghcr.io/cross-rs/*' --format '  {{.Repository}}:{{.Tag}}  {{.Size}}'
