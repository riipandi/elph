#!/usr/bin/env bash
# ---------------------------------------------------------------------------
# Elph -- Install script
#
# Usage:
#   curl -fsSL https://elph.space/install.sh | bash
#   curl -fsSL https://elph.space/install.sh | bash -s -- --pre
#   curl -fsSL https://raw.githubusercontent.com/riipandi/elph/main/install.sh | bash
#
# Options:
#   --version <tag>      Pin a specific version (default: latest)
#   --pre                Use the latest pre-release (includes alpha/beta/rc)
#   --home <dir>         Elph home directory (default: ~/.elph)
#   --install-dir <dir>  Binary install directory (default: ~/.local/bin)
#   --dry-run            Print what would happen without downloading
#   --help               Show this help
#
# Also via env vars: ELPH_VERSION, ELPH_PRE_RELEASE, ELPH_HOME, ELPH_INSTALL_DIR
set -euo pipefail

REPO_OWNER="riipandi"
REPO_NAME="elph"
BINARY_NAME="elph"

ELPH_VERSION="${ELPH_VERSION:-}"
ELPH_HOME="${ELPH_HOME:-"${HOME}/.elph"}"
ELPH_INSTALL_DIR="${ELPH_INSTALL_DIR:-"${HOME}/.local/bin"}"
ELPH_DRY_RUN="${ELPH_DRY_RUN:-}"
ELPH_PRE_RELEASE=

# -- Arg parsing ----
while [[ $# -gt 0 ]]; do
  case "$1" in
  --version)
    ELPH_VERSION="$2"
    shift 2
    ;;
  --version=*)
    ELPH_VERSION="${1#*=}"
    shift
    ;;
  --home)
    ELPH_HOME="$2"
    shift 2
    ;;
  --home=*)
    ELPH_HOME="${1#*=}"
    shift
    ;;
  --install-dir)
    ELPH_INSTALL_DIR="$2"
    shift 2
    ;;
  --install-dir=*)
    ELPH_INSTALL_DIR="${1#*=}"
    shift
    ;;
  --dry-run)
    ELPH_DRY_RUN=1
    shift
    ;;
  --pre)
    ELPH_PRE_RELEASE=1
    shift
    ;;
  --help)
    sed -n '3,18p' "$0"
    exit 0
    ;;
  *)
    echo "Unknown option: $1" >&2
    echo "Usage: \$0 [--version <tag>] [--pre] [--home <dir>] [--install-dir <dir>] [--dry-run] [--help]" >&2
    exit 1
    ;;
  esac
done

# -- Colors ----
if [[ -t 1 && "${NO_COLOR:-}" != "true" ]]; then
  BOLD="\033[1m"
  DIM="\033[2m"
  GREEN="\033[32m"
  YELLOW="\033[33m"
  RED="\033[31m"
  CYAN="\033[36m"
  RESET="\033[0m"
else
  BOLD=""
  DIM=""
  GREEN=""
  YELLOW=""
  RED=""
  CYAN=""
  RESET=""
fi

info() { printf "${GREEN}==>${RESET}${BOLD} %s${RESET}\n" "$*"; }
warn() { printf "${YELLOW}==>${RESET}${BOLD} %s${RESET}\n" "$*" >&2; }
err() { printf "${RED}==>${RESET}${BOLD} %s${RESET}\n" "$*" >&2; }
step() { printf "${CYAN}==>${RESET} %s\n" "$*" >&2; }
die() {
  err "$1"
  exit 1
}

# -- Platform ----
detect_os() {
  local u
  u="$(uname -s)"
  case "${u}" in
  Linux*) echo "linux" ;;
  Darwin*) echo "darwin" ;;
  *) die "Unsupported OS: ${u}. Only Linux and macOS are supported." ;;
  esac
}

detect_arch() {
  local u
  u="$(uname -m)"
  case "${u}" in
  x86_64 | amd64) echo "amd64" ;;
  aarch64 | arm64) echo "arm64" ;;
  *) die "Unsupported arch: ${u}. Only amd64 and arm64 are supported." ;;
  esac
}

OS="$(detect_os)"
ARCH="$(detect_arch)"

# -- Dependencies ----
command -v curl >/dev/null 2>&1 || die "Required: curl (not found)"
command -v tar >/dev/null 2>&1 || die "Required: tar (not found)"

if command -v shasum >/dev/null 2>&1; then
  SHASUM="shasum -a 256"
elif command -v sha256sum >/dev/null 2>&1; then
  SHASUM="sha256sum"
else
  die "Required: shasum or sha256sum (not found)"
fi

# -- Version ----
resolve_version() {
  if [[ -n "${ELPH_VERSION}" ]]; then
    echo "${ELPH_VERSION}"
    return
  fi

  local tag_ref="${GITHUB_REF:-}"
  if [[ "${tag_ref}" == refs/tags/v* ]]; then
    echo "${tag_ref#refs/tags/}"
    return
  fi

  step "Fetching latest release info..."
  local api_url="https://api.github.com/repos/${REPO_OWNER}/${REPO_NAME}/releases"
  local tag=""

  if [[ -n "${ELPH_PRE_RELEASE}" ]]; then
    tag="$(curl -fsSL "${api_url}?per_page=10" 2>/dev/null |
      grep -m1 '"tag_name":' |
      sed 's/.*"tag_name": "\(.*\)",.*/\1/')"
  else
    # Stable only: use the /latest endpoint (GitHub excludes pre-releases).
    tag="$(curl -fsSL "${api_url}/latest" 2>/dev/null |
      grep '"tag_name":' |
      sed 's/.*"tag_name": "\(.*\)",.*/\1/' || true)"
  fi

  if [[ -z "${tag}" ]]; then
    tag="v0.0.1"
    warn "Could not determine latest release; falling back to ${tag}"
  fi

  echo "${tag}"
}

VERSION="$(resolve_version)"
VERSION_NUM="${VERSION#v}"

info "Elph ${VERSION} -- ${OS}/${ARCH}$([ -n "${ELPH_PRE_RELEASE}" ] && echo ' (pre-release)')"

# -- URLs ----
BASE_URL="https://github.com/${REPO_OWNER}/${REPO_NAME}/releases/download/${VERSION}"
ARCHIVE_NAME="${BINARY_NAME}_${VERSION_NUM}_${OS}_${ARCH}.tar.gz"
ARCHIVE_URL="${BASE_URL}/${ARCHIVE_NAME}"
CHECKSUM_URL="${BASE_URL}/checksums.txt"

# -- Dry run ----
if [[ -n "${ELPH_DRY_RUN}" ]]; then
  info "[DRY RUN] Would install:"
  echo
  printf "  ${BOLD}Version:${RESET}     %s\n" "${VERSION}"
  if [[ -n "${ELPH_PRE_RELEASE}" ]]; then
    printf "  ${BOLD}Channel:${RESET}      pre-release\n"
  fi
  printf "  ${BOLD}Platform:${RESET}    %s/%s\n" "${OS}" "${ARCH}"
  printf "  ${BOLD}Archive:${RESET}     %s\n" "${ARCHIVE_URL}"
  printf "  ${BOLD}Checksum:${RESET}    %s\n" "${CHECKSUM_URL}"
  printf "  ${BOLD}Elph home:${RESET}   %s\n" "${ELPH_HOME}"
  printf "  ${BOLD}Install to:${RESET}  %s\n" "${ELPH_INSTALL_DIR}"
  echo
  info "[DRY RUN] Done. Set ELPH_VERSION to pin a version."
  exit 0
fi

# -- Download ----
TMPDIR="$(mktemp -d)"
trap 'rm -rf "${TMPDIR}"' EXIT

step "Downloading ${ARCHIVE_NAME}..."
curl -fL# "${ARCHIVE_URL}" -o "${TMPDIR}/${ARCHIVE_NAME}" ||
  die "Download failed: ${ARCHIVE_URL}"
echo

step "Downloading checksums..."
curl -fsSL "${CHECKSUM_URL}" -o "${TMPDIR}/checksums.txt" ||
  warn "Checksum file not found; skipping verification"

ARCHIVE_SUM=
if [[ -f "${TMPDIR}/checksums.txt" ]]; then
  ARCHIVE_SUM="$(grep "${ARCHIVE_NAME}" "${TMPDIR}/checksums.txt" | awk '{print $1}')"

  if [[ -z "${ARCHIVE_SUM}" ]]; then
    die "No checksum found for ${ARCHIVE_NAME} in checksums.txt"
  fi

  step "Verifying SHA256 checksum..."

  if ${SHASUM} -c "${TMPDIR}/checksums.txt" --ignore-missing 2>/dev/null; then
    :
  else
    ac="$(${SHASUM} "${TMPDIR}/${ARCHIVE_NAME}" | awk '{print $1}')"
    if [[ "${ARCHIVE_SUM}" != "${ac}" ]]; then
      die "Checksum mismatch for ${ARCHIVE_NAME} -- expected ${ARCHIVE_SUM}, got ${ac}"
    fi
  fi
fi

# -- Extract ----
step "Extracting archive..."
tar -xzf "${TMPDIR}/${ARCHIVE_NAME}" -C "${TMPDIR}"

BINARY_PATH="${TMPDIR}/${OS}_${ARCH}/${BINARY_NAME}"
if [[ ! -f "${BINARY_PATH}" ]]; then
  BINARY_PATH="${TMPDIR}/${BINARY_NAME}"
fi
if [[ ! -f "${BINARY_PATH}" ]]; then
  die "Binary not found in archive (looked for ${BINARY_PATH})"
fi

chmod +x "${BINARY_PATH}"

"${BINARY_PATH}" version >/dev/null 2>&1 ||
  warn "Binary may not run correctly; '${BINARY_NAME} version' failed."

# -- Install ----
mkdir -p "${ELPH_HOME}" "${ELPH_INSTALL_DIR}"
INSTALL_PATH="${ELPH_INSTALL_DIR}/${BINARY_NAME}"
cp "${BINARY_PATH}" "${INSTALL_PATH}"

# -- Post-install ----
echo
if [[ -n "${ARCHIVE_SUM}" ]]; then
  printf "    ${DIM}Checksum:${RESET}    ${ARCHIVE_SUM}\n"
fi
printf "    ${DIM}Binary:${RESET}      ${INSTALL_PATH}\n"
printf "    ${DIM}Elph home:${RESET}   ${ELPH_HOME}\n"
printf "    ${DIM}Size:${RESET}       %s\n" "$(du -sh "${INSTALL_PATH}" | cut -f1)"

if ! echo "${PATH}" | tr ':' '\n' | grep -qx "${ELPH_INSTALL_DIR}"; then
  warn "${ELPH_INSTALL_DIR} is not in your PATH"
  echo
  printf "  Add this to your shell config (e.g. ~/.zshrc, ~/.bashrc):\n"
  printf "  ${CYAN}export PATH=\"\${HOME}/.local/bin:\${PATH}\"${RESET}\n"
fi

echo
step "Run '${BINARY_NAME} --help' to get started."
step "Visit https://elph.space or https://github.com/${REPO_OWNER}/${REPO_NAME} for docs."
echo
