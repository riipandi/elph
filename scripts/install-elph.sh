#!/usr/bin/env bash
# ---------------------------------------------------------------------------
# Elph -- Install script
#
# Usage:
#   curl -fsSL https://elph.space/elph/install.sh | bash
#   curl -fsSL https://elph.space/elph/install.sh | bash -s -- --canary
#   curl -fsSL https://raw.githubusercontent.com/riipandi/elph/main/scripts/install-elph.sh | bash
#
# Options:
#   --version <tag>      Pin a specific version (default: latest elph-v* release)
#   --canary             Use the latest elph pre-release
#   --install-dir <dir>  Binary install directory (default: ~/.local/bin)
#   --dry-run            Print what would happen without downloading
#   --help               Show this help
#
# Also via env vars: ELPH_VERSION, ELPH_CANARY, ELPH_INSTALL_DIR
set -euo pipefail

INSTALL_APP="elph"
INSTALL_APP_TITLE="Elph"
INSTALL_ENV_PREFIX="ELPH"
INSTALL_REPO_OWNER="riipandi"
INSTALL_REPO_NAME="elph"

show_install_help() {
    cat <<'EOF'
Elph -- Install script

Usage:
  curl -fsSL https://elph.space/elph/install.sh | bash
  curl -fsSL https://elph.space/elph/install.sh | bash -s -- --canary

Options:
  --version <tag>      Pin a specific version (default: latest elph-v* release)
  --canary             Use the latest elph pre-release
  --install-dir <dir>  Binary install directory (default: ~/.local/bin)
  --dry-run            Print what would happen without downloading
  --help               Show this help

Also via env vars: ELPH_VERSION, ELPH_CANARY, ELPH_INSTALL_DIR
EOF
}

_install_env() {
    local key="$1"
    local default="${2:-}"
    local name="${INSTALL_ENV_PREFIX}_${key}"
    local value="${!name:-}"
    if [[ -n "$value" ]]; then
        echo "$value"
    else
        echo "$default"
    fi
}

INSTALL_VERSION="$(_install_env VERSION)"
INSTALL_INSTALL_DIR="$(_install_env INSTALL_DIR "${HOME}/.local/bin")"
INSTALL_DRY_RUN="$(_install_env DRY_RUN)"
INSTALL_CANARY="$(_install_env CANARY)"

install_colors() {
    if [[ -t 1 && "${NO_COLOR:-}" != "true" ]]; then
        INSTALL_BOLD="\033[1m"
        INSTALL_DIM="\033[2m"
        INSTALL_GREEN="\033[32m"
        INSTALL_YELLOW="\033[33m"
        INSTALL_RED="\033[31m"
        INSTALL_CYAN="\033[36m"
        INSTALL_RESET="\033[0m"
    else
        INSTALL_BOLD=""
        INSTALL_DIM=""
        INSTALL_GREEN=""
        INSTALL_YELLOW=""
        INSTALL_RED=""
        INSTALL_CYAN=""
        INSTALL_RESET=""
    fi
}

install_info() { printf "${INSTALL_GREEN}==>${INSTALL_RESET}${INSTALL_BOLD} %s${INSTALL_RESET}\n" "$*"; }
install_warn() { printf "${INSTALL_YELLOW}==>${INSTALL_RESET}${INSTALL_BOLD} %s${INSTALL_RESET}\n" "$*" >&2; }
install_err() { printf "${INSTALL_RED}==>${INSTALL_RESET}${INSTALL_BOLD} %s${INSTALL_RESET}\n" "$*" >&2; }
install_step() { printf "${INSTALL_CYAN}==>${INSTALL_RESET} %s\n" "$*" >&2; }
install_die() {
    install_err "$1"
    exit 1
}

install_parse_args() {
    while [[ $# -gt 0 ]]; do
        case "$1" in
        --version)
            INSTALL_VERSION="$2"
            shift 2
            ;;
        --version=*)
            INSTALL_VERSION="${1#*=}"
            shift
            ;;
        --install-dir)
            INSTALL_INSTALL_DIR="$2"
            shift 2
            ;;
        --install-dir=*)
            INSTALL_INSTALL_DIR="${1#*=}"
            shift
            ;;
        --dry-run)
            INSTALL_DRY_RUN=1
            shift
            ;;
        --canary)
            INSTALL_CANARY=1
            shift
            ;;
        --help)
            show_install_help
            exit 0
            ;;
        *)
            install_die "Unknown option: $1"
            ;;
        esac
    done
}

install_detect_platform() {
    case "$(uname -s)" in
    Linux)
        if [[ -f /etc/alpine-release ]]; then
            echo "linux-musl"
        else
            echo "linux-glibc"
        fi
        ;;
    Darwin) echo "macos" ;;
    *) install_die "Unsupported OS: $(uname -s). Only Linux and macOS are supported." ;;
    esac
}

install_detect_arch() {
    case "$(uname -m)" in
    x86_64 | amd64) echo "x86_64" ;;
    aarch64 | arm64) echo "arm64" ;;
    *) install_die "Unsupported arch: $(uname -m). Only x86_64 and arm64 are supported." ;;
    esac
}

install_normalize_tag() {
    local version="$1"
    local prefix="${INSTALL_APP}-v"

    if [[ -z "$version" || "$version" == "latest" ]]; then
        return 1
    fi
    if [[ "$version" == "${prefix}"* ]]; then
        echo "$version"
    else
        echo "${prefix}${version#v}"
    fi
}

install_resolve_tag() {
    local canary="${1:-0}"
    local prefix="${INSTALL_APP}-v"
    local api_url="https://api.github.com/repos/${INSTALL_REPO_OWNER}/${INSTALL_REPO_NAME}/releases?per_page=100"
    local json tag

    install_step "Resolving latest ${INSTALL_APP} release..."
    json="$(curl -fsSL "${api_url}")" ||
        install_die "Failed to fetch GitHub releases from ${api_url}"

    tag="$(printf '%s' "${json}" | python3 -c '
import json
import re
import sys

app = sys.argv[1]
canary = sys.argv[2] == "1"
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
    if canary:
        if not release.get("prerelease", False):
            continue
    elif release.get("prerelease", False):
        continue
    tags.append(tag_name)

if not tags:
    sys.exit(1)

tags.sort(key=ver_key, reverse=True)
print(tags[0])
' "${INSTALL_APP}" "${canary}")" || tag=""

    if [[ -z "${tag}" ]]; then
        install_die "No ${INSTALL_APP} releases found on GitHub (prefix: ${prefix}*)"
    fi

    echo "${tag}"
}

install_resolve_version() {
    local tag=""

    if tag="$(install_normalize_tag "${INSTALL_VERSION}" 2>/dev/null)"; then
        echo "$tag"
        return
    fi

    tag="$(install_resolve_tag "${INSTALL_CANARY}")"
    echo "$tag"
}

install_require_tools() {
    command -v curl >/dev/null 2>&1 || install_die "Required: curl (not found)"
    command -v tar >/dev/null 2>&1 || install_die "Required: tar (not found)"
    command -v python3 >/dev/null 2>&1 || install_die "Required: python3 (not found)"

    if command -v shasum >/dev/null 2>&1; then
        INSTALL_SHASUM="shasum -a 256"
    elif command -v sha256sum >/dev/null 2>&1; then
        INSTALL_SHASUM="sha256sum"
    else
        install_die "Required: shasum or sha256sum (not found)"
    fi
}

install_run() {
    install_colors
    install_parse_args "$@"
    install_require_tools

    local platform arch archive_name archive_url checksum_url version_tag version_num
    platform="$(install_detect_platform)"
    arch="$(install_detect_arch)"
    version_tag="$(install_resolve_version)"
    if [[ -z "${version_tag}" ]]; then
        install_die "Failed to resolve ${INSTALL_APP} version"
    fi
    version_num="${version_tag#${INSTALL_APP}-v}"
    archive_name="${INSTALL_APP}-${platform}-${arch}.tar.gz"
    archive_url="https://github.com/${INSTALL_REPO_OWNER}/${INSTALL_REPO_NAME}/releases/download/${version_tag}/${archive_name}"
    checksum_url="https://github.com/${INSTALL_REPO_OWNER}/${INSTALL_REPO_NAME}/releases/download/${version_tag}/SHA256SUMS"

    install_info "${INSTALL_APP_TITLE} ${version_tag} -- ${platform}/${arch}$([ -n "${INSTALL_CANARY}" ] && echo ' (pre-release)')"

    if [[ -n "${INSTALL_DRY_RUN}" ]]; then
        install_info "[DRY RUN] Would install:"
        echo
        printf "  ${INSTALL_BOLD}Tag:${INSTALL_RESET}        %s\n" "${version_tag}"
        printf "  ${INSTALL_BOLD}Version:${INSTALL_RESET}     %s\n" "${version_num}"
        if [[ -n "${INSTALL_CANARY}" ]]; then
            printf "  ${INSTALL_BOLD}Channel:${INSTALL_RESET}     pre-release\n"
        fi
        printf "  ${INSTALL_BOLD}Platform:${INSTALL_RESET}    %s/%s\n" "${platform}" "${arch}"
        printf "  ${INSTALL_BOLD}Archive:${INSTALL_RESET}     %s\n" "${archive_url}"
        printf "  ${INSTALL_BOLD}Checksum:${INSTALL_RESET}    %s\n" "${checksum_url}"
        printf "  ${INSTALL_BOLD}Install to:${INSTALL_RESET}  %s\n" "${INSTALL_INSTALL_DIR}"
        echo
        install_info "[DRY RUN] Done. Pin with --version or ${INSTALL_ENV_PREFIX}_VERSION."
        exit 0
    fi

    local tmpdir archive_sum binary_path install_path
    tmpdir="$(mktemp -d)"
    trap 'rm -rf "${tmpdir}"' EXIT

    install_step "Downloading ${archive_name}..."
    curl -fL# "${archive_url}" -o "${tmpdir}/${archive_name}" ||
        install_die "Download failed: ${archive_url}"
    echo

    install_step "Downloading SHA256SUMS..."
    curl -fsSL "${checksum_url}" -o "${tmpdir}/SHA256SUMS" ||
        install_warn "Checksum file not found; skipping verification"

    archive_sum=""
    if [[ -f "${tmpdir}/SHA256SUMS" ]]; then
        archive_sum="$(grep " ${archive_name}\$" "${tmpdir}/SHA256SUMS" | awk '{print $1}')"
        if [[ -z "${archive_sum}" ]]; then
            install_die "No checksum found for ${archive_name} in SHA256SUMS"
        fi

        install_step "Verifying SHA256 checksum..."
        local actual
        actual="$(${INSTALL_SHASUM} "${tmpdir}/${archive_name}" | awk '{print $1}')"
        if [[ "${archive_sum}" != "${actual}" ]]; then
            install_die "Checksum mismatch for ${archive_name} -- expected ${archive_sum}, got ${actual}"
        fi
    fi

    install_step "Extracting archive..."
    tar -xzf "${tmpdir}/${archive_name}" -C "${tmpdir}"

    binary_path="${tmpdir}/${INSTALL_APP}"
    [[ -f "${binary_path}" ]] || install_die "Binary not found in archive (expected ${INSTALL_APP})"

    chmod +x "${binary_path}"
    "${binary_path}" version >/dev/null 2>&1 ||
        install_warn "Binary may not run correctly; '${INSTALL_APP} version' failed."

    mkdir -p "${INSTALL_INSTALL_DIR}"
    install_path="${INSTALL_INSTALL_DIR}/${INSTALL_APP}"
    cp "${binary_path}" "${install_path}"

    echo
    if [[ -n "${archive_sum}" ]]; then
        printf "    ${INSTALL_DIM}Checksum:${INSTALL_RESET}    ${archive_sum}\n"
    fi
    printf "    ${INSTALL_DIM}Binary:${INSTALL_RESET}      ${install_path}\n"
    printf "    ${INSTALL_DIM}Size:${INSTALL_RESET}        %s\n" "$(du -sh "${install_path}" | cut -f1)"

    if ! echo "${PATH}" | tr ':' '\n' | grep -qx "${INSTALL_INSTALL_DIR}"; then
        install_warn "${INSTALL_INSTALL_DIR} is not in your PATH"
        echo
        printf "  Add this to your shell config (e.g. ~/.zshrc, ~/.bashrc):\n"
        printf "  ${INSTALL_CYAN}export PATH=\"\${HOME}/.local/bin:\${PATH}\"${INSTALL_RESET}\n"
    fi

    echo
    install_step "Run '${INSTALL_APP} --help' to get started."
    install_step "Visit https://elph.space/${INSTALL_APP} or https://github.com/${INSTALL_REPO_OWNER}/${INSTALL_REPO_NAME} for docs."
    echo
}

install_run "$@"
