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

cross_print_plan() {
  local -a targets=("$@")
  local target tool os_label
  os_label="$(cross_host_label)"

  echo "Host: ${os_label}"
  echo "Plan:"

  for target in "${targets[@]}"; do
    tool="$(cross_tool_for "$target")"
    case "$tool" in
    cargo) echo "  cargo  ${target}" ;;
    cross) echo "  cross  ${target}" ;;
    skip) echo "  skip   ${target}" ;;
    esac
  done
  echo
}
