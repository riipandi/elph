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
platform_note=""
if [[ -n "${CROSS_DOCKER_PLATFORM:-}" ]]; then
  docker_platform=(--platform "$CROSS_DOCKER_PLATFORM")
  platform_note="${CROSS_DOCKER_PLATFORM}"
elif [[ "$(cross_host_rust_arch)" == "aarch64" ]]; then
  docker_platform=(--platform linux/amd64)
  platform_note="linux/amd64"
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

banner="Cross-pull  tag:${tag}"
if [[ -n "$platform_note" ]]; then
  banner="${banner}  ·  ${platform_note}"
fi
cross_log_banner "$banner"
echo

if ((${#docker_targets[@]} == 0)); then
  echo "No cross-rs images needed on this host."
  exit 0
fi

pull_image() {
  local target="$1"
  local image="ghcr.io/cross-rs/${target}:${tag}"
  if docker pull --quiet "${docker_platform[@]}" "$image" >/dev/null 2>&1; then
    printf '  %-34s  pulled\n' "$target"
    return 0
  fi
  if [[ "$tag" != "main" ]]; then
    local fallback="ghcr.io/cross-rs/${target}:main"
    if docker pull --quiet "${docker_platform[@]}" "$fallback" >/dev/null 2>&1; then
      printf '  %-34s  pulled (main)\n' "$target"
      return 0
    fi
  fi
  printf '  %-34s  failed\n' "$target" >&2
  return 1
}

for target in "${docker_targets[@]}"; do
  pull_image "$target" || exit 1
done

if ((${#skipped_targets[@]} > 0)); then
  echo
  echo "Skipped"
  for target in "${skipped_targets[@]}"; do
    printf '  %-34s  no ghcr.io image\n' "$target"
  done
fi

echo
echo "Cached"
docker image ls 'ghcr.io/cross-rs/*' --format '  {{.Repository}}:{{.Tag}}  {{.Size}}' | sort
