#!/usr/bin/env bash
# ---------------------------------------------------------------------------
# owly -- Install script
#
# Usage:
#   curl -fsSL https://elph.space/owly/install.sh | bash
#   curl -fsSL https://elph.space/owly/install.sh | bash -s -- --canary
#   curl -fsSL https://raw.githubusercontent.com/riipandi/elph/main/scripts/install-owly.sh | bash
#
# Options:
#   --version <tag>      Pin a specific version (default: latest)
#   --canary             Use the canary release (pre-release)
#   --home <dir>         owly home directory (default: ~/.owly)
#   --install-dir <dir>  Binary install directory (default: ~/.local/bin)
#   --dry-run            Print what would happen without downloading
#   --help               Show this help
#
# Also via env vars: OWLY_VERSION, OWLY_CANARY, OWLY_HOME, OWLY_INSTALL_DIR
set -euo pipefail
