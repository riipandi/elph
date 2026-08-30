#!/bin/sh
# herdr integration hook for elph
# Reports elph lifecycle state to the local Herdr pane over its socket API.
# Installed at CONFIG_DIR/hooks/herdr-agent-state.sh and registered from
# CONFIG_DIR/hooks.json. No-op outside a Herdr pane (HERDR_ENV != 1).
#
# Fields: HERDR_ENV=1, HERDR_PANE_ID, HERDR_SOCKET_PATH, HERDR_BIN_PATH are
# injected by Herdr into pane processes and forwarded to hook children by elph.
# ELPH_SESSION_ID and ELPH_PROJECT_DIR are forwarded when set.
# NOTE on seq and source: reports use a monotonic nanosecond timestamp as
# `--seq`. Herdr rejects a report whose seq is not greater than the last
# accepted seq for the same source. `SOURCE` is configurable (default
# `elph-hooks`); give each elph session its own source (e.g.
# `elph-hooks-<session-id>`) so its sequence space starts fresh. A source that
# was `release`d cannot re-acquire authority with a low seq — once released,
# use a new source or restart Herdr.

# Guard: only active inside a Herdr pane
if [ "${HERDR_ENV:-}" != "1" ]; then
    exit 0
fi
if [ -z "${HERDR_PANE_ID:-}" ]; then
    exit 0
fi

report() {
    # $1 = state (idle|working|blocked|unknown)
    # $2 = message (optional)
    [ -n "${HERDR_BIN_PATH:-}" ] || return 0
    [ -x "${HERDR_BIN_PATH:-}" ] || return 0

    # Unique per-invocation sequence: monotonic nanosecond timestamp. Herdr
    # rejects reports whose seq is not greater than the last accepted seq for
    # the same source. A per-session source keeps a fresh sequence space.
    seq="$(date +%s%N 2>/dev/null || echo 0)"
    if [ -n "${2:-}" ]; then
        "${HERDR_BIN_PATH}" pane report-agent "${HERDR_PANE_ID}" \
            --source "${SOURCE:-elph-hooks}" \
            --agent "elph" \
            --state "$1" \
            --message "$2" \
            --seq "$seq" \
            >/dev/null 2>&1
    else
        "${HERDR_BIN_PATH}" pane report-agent "${HERDR_PANE_ID}" \
            --source "${SOURCE:-elph-hooks}" \
            --agent "elph" \
            --state "$1" \
            --seq "$seq" \
            >/dev/null 2>&1
    fi
}

# Consume stdin JSON (one hook event per invocation from elph)
payload=""
while IFS= read -r line; do
    payload="${payload}${line}"
done

event=$(printf '%s' "$payload" | sed -n 's/.*"event"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p')

case "$event" in
    beforeAgent|userPromptSubmit|preToolUse)
        # Agent starts processing a turn → working
        report "working"
        ;;
    stop)
        # Turn settled → idle
        report "idle"
        ;;
    sessionStart)
        # Session initialized → idle; report native session reference for restore
        if [ -n "${ELPH_SESSION_ID:-}" ]; then
            report "idle"
            seq="$(date +%s%N 2>/dev/null || echo 0)"
            [ -n "${HERDR_BIN_PATH:-}" ] && [ -x "${HERDR_BIN_PATH:-}" ] &&
                "${HERDR_BIN_PATH}" pane report-agent-session "${HERDR_PANE_ID}" \
                    --source "${SOURCE:-elph-hooks}" \
                    --agent "elph" \
                    --agent-session-id "${ELPH_SESSION_ID}" \
                    --seq "$seq" \
                    >/dev/null 2>&1
        else
            report "idle"
        fi
        ;;
esac

exit 0