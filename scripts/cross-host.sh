# Host-aware cross-compilation helpers. Source from other scripts.

cross_host_os() {
  uname -s
}

cross_host_arch() {
  uname -m
}

# Map machine hardware name to Rust arch prefix.
cross_host_rust_arch() {
  case "$(cross_host_arch)" in
  x86_64 | amd64) echo "x86_64" ;;
  arm64 | aarch64) echo "aarch64" ;;
  *) echo "$(cross_host_arch)" ;;
  esac
}

cross_host_is_alpine() {
  [[ -f /etc/alpine-release ]]
}

cross_host_is_windows() {
  local os
  os="$(cross_host_os)"
  [[ "$os" == MINGW* || "$os" == MSYS* || "$os" == CYGWIN* || "$os" == Windows* ]]
}

# ghcr.io/cross-rs/<target> is not published for every Rust triple.
cross_image_published() {
  case "$1" in
  aarch64-pc-windows-msvc) return 1 ;;
  *) return 0 ;;
  esac
}

# Print: cargo | cross | skip
cross_tool_for() {
  local target="$1"
  local os arch
  os="$(cross_host_os)"
  arch="$(cross_host_rust_arch)"

  case "$target" in
  *-apple-darwin)
    if [[ "$os" == "Darwin" ]]; then
      echo "cargo"
    else
      echo "skip"
    fi
    ;;
  *-pc-windows-*)
    if cross_host_is_windows; then
      case "$target" in
      "${arch}"-pc-windows-*)
        echo "cargo"
        ;;
      *)
        echo "cross"
        ;;
      esac
    else
      echo "cross"
    fi
    ;;
  *-unknown-linux-gnu)
    if [[ "$os" == "Linux" && "${target%%-*}" == "$arch" ]]; then
      echo "cargo"
    else
      echo "cross"
    fi
    ;;
  *-unknown-linux-musl)
    if [[ "$os" == "Linux" && "${target%%-*}" == "$arch" ]] && cross_host_is_alpine; then
      echo "cargo"
    else
      echo "cross"
    fi
    ;;
  *)
    echo "cross"
    ;;
  esac
}

cross_host_label() {
  local os arch alpine=""
  os="$(cross_host_os)"
  arch="$(cross_host_arch)"
  if cross_host_is_alpine; then
    alpine=" (Alpine)"
  fi
  printf '%s %s%s' "$os" "$arch" "$alpine"
}

cross_fmt_bytes() {
  local bytes="$1"
  if ((bytes >= 1048576)); then
    printf '%dMB' $((bytes / 1048576))
  elif ((bytes >= 1024)); then
    printf '%dKB' $((bytes / 1024))
  else
    printf '%dB' "$bytes"
  fi
}

cross_log_banner() {
  printf '%s  ·  %s\n' "$1" "$(cross_host_label)"
}

cross_print_plan() {
  local -a targets=("$@")
  local target tool

  echo "Plan"
  for target in "${targets[@]}"; do
    tool="$(cross_tool_for "$target")"
    if [[ "$tool" == "cross" ]] && ! cross_image_published "$target"; then
      tool="skip"
    fi
    printf '  %-5s  %s\n' "$tool" "$target"
  done
  echo
}

cross_log_target() {
  local target="$1"
  local tool="$2"
  printf '► %s  (%s)\n' "$target" "$tool"
}

cross_log_artifact() {
  local pkg="$1"
  local binary_name="$2"
  local artifact_name="$3"
  local bytes="$4"
  local checksum="$5"
  printf '  %-5s  %-26s  %-34s  %s  %s\n' \
    "$pkg" "$binary_name" "$artifact_name" "$(cross_fmt_bytes "$bytes")" "$checksum"
}

cross_print_release_tree() {
  local root="$1"
  local dir name count

  echo "Release"
  for dir in archives binaries; do
    count=0
    while IFS= read -r name; do
      [[ -z "$name" || "$name" == "SHA256SUMS" ]] && continue
      if ((count == 0)); then
        printf '  %s/\n' "$dir"
      fi
      printf '    %s\n' "$name"
      count=$((count + 1))
    done < <(ls -1 "${root}/release/${dir}" 2>/dev/null | sort)
    if ((count == 0)); then
      printf '  %s/  (empty)\n' "$dir"
    fi
  done
}
