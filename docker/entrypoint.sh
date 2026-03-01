#!/usr/bin/env bash
# ClawDen Runtime Entrypoint
#
# Environment variables (set by `clawden` CLI):
#   RUNTIME  — runtime to launch (zeroclaw, picoclaw, openclaw, nanoclaw, openfang)
#   TOOLS    — comma-separated list of tools to set up (git, http, browser, gui)
#
# This script:
#   1. Sources tool setup scripts for each requested tool
#   2. Applies runtime-specific defaults
#   3. Execs into the runtime binary/process

set -euo pipefail

RUNTIME="${RUNTIME:-}"
TOOLS="${TOOLS:-}"

if [ -z "$RUNTIME" ]; then
    echo "Error: RUNTIME environment variable must be set (e.g., zeroclaw, picoclaw, openclaw, nanoclaw)" >&2
    exit 1
fi

# --- Tool setup ---
if [ -n "$TOOLS" ]; then
    IFS=',' read -ra TOOL_LIST <<< "$TOOLS"
    for tool in "${TOOL_LIST[@]}"; do
        tool="$(echo "$tool" | xargs)"  # trim whitespace
        setup_script="/opt/clawden/tools/${tool}/setup.sh"
        if [ -f "$setup_script" ]; then
            echo "[clawden] Setting up tool: ${tool}"
            # shellcheck disable=SC1090
            source "$setup_script"
        else
            echo "[clawden] Warning: Unknown tool '${tool}', skipping" >&2
        fi
    done
fi

# --- Runtime launch ---
# Runtimes are installed by `clawden-cli install` into $HOME/.clawden/runtimes/
# Each runtime has a versioned dir with a `current` symlink and a launcher/binary.
CLAWDEN_RUNTIMES="${HOME}/.clawden/runtimes"

echo "[clawden] Starting runtime: ${RUNTIME}"

case "$RUNTIME" in
    zeroclaw|picoclaw|openclaw|nanoclaw)
        LAUNCHER="${CLAWDEN_RUNTIMES}/${RUNTIME}/current/${RUNTIME}"
        if [ ! -x "$LAUNCHER" ]; then
            echo "Error: ${RUNTIME} not found at ${LAUNCHER}" >&2
            echo "Run: clawden-cli install ${RUNTIME}" >&2
            exit 1
        fi
        exec "$LAUNCHER" "$@"
        ;;
    # DISABLED: OpenFang temporarily removed
    # openfang)
    #     BINARY="/opt/clawden/runtimes/openfang"
    #     if [ ! -x "$BINARY" ]; then
    #         echo "Error: OpenFang binary not found at ${BINARY}" >&2
    #         exit 1
    #     fi
    #     exec "$BINARY" "$@"
    #     ;;
    *)
        echo "Error: Unknown runtime '${RUNTIME}'" >&2
        echo "Supported runtimes: zeroclaw, picoclaw, openclaw, nanoclaw" >&2
        exit 1
        ;;
esac
