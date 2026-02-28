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
    echo "Error: RUNTIME environment variable must be set (e.g., zeroclaw, picoclaw, openclaw, nanoclaw, openfang)" >&2
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
echo "[clawden] Starting runtime: ${RUNTIME}"

case "$RUNTIME" in
    zeroclaw)
        BINARY="/opt/clawden/runtimes/zeroclaw"
        if [ ! -x "$BINARY" ]; then
            echo "Error: ZeroClaw binary not found at ${BINARY}" >&2
            exit 1
        fi
        exec "$BINARY" "$@"
        ;;
    picoclaw)
        BINARY="/opt/clawden/runtimes/picoclaw"
        if [ ! -x "$BINARY" ]; then
            echo "Error: PicoClaw binary not found at ${BINARY}" >&2
            exit 1
        fi
        exec "$BINARY" "$@"
        ;;
    openclaw)
        # OpenClaw is installed globally via npm; the symlink at
        # /opt/clawden/runtimes/openclaw points to the npm package dir.
        if command -v openclaw &>/dev/null; then
            exec openclaw "$@"
        fi
        APP_DIR="/opt/clawden/runtimes/openclaw"
        if [ ! -d "$APP_DIR" ]; then
            echo "Error: OpenClaw app not found at ${APP_DIR}" >&2
            exit 1
        fi
        cd "$APP_DIR"
        exec node index.js "$@"
        ;;
    nanoclaw)
        APP_DIR="/opt/clawden/runtimes/nanoclaw"
        if [ ! -d "$APP_DIR" ]; then
            echo "Error: NanoClaw app not found at ${APP_DIR}" >&2
            exit 1
        fi
        cd "$APP_DIR"
        # NanoClaw is a TypeScript app — use npm start which handles transpilation
        exec npm start -- "$@"
        ;;
    openfang)
        BINARY="/opt/clawden/runtimes/openfang"
        if [ ! -x "$BINARY" ]; then
            echo "Error: OpenFang binary not found at ${BINARY}" >&2
            exit 1
        fi
        exec "$BINARY" "$@"
        ;;
    *)
        echo "Error: Unknown runtime '${RUNTIME}'" >&2
        echo "Supported runtimes: zeroclaw, picoclaw, openclaw, nanoclaw, openfang" >&2
        exit 1
        ;;
esac
