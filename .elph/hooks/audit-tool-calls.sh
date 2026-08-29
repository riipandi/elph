#!/bin/sh
set -eu

# Hook commands run with the active project as their working directory.
# Keep stdout empty: this hook observes the call without changing its outcome.
cat >>.elph/hook-audit.log
