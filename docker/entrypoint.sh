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
TOOLS_STATE_DIR="/run/clawden"
if ! mkdir -p "$TOOLS_STATE_DIR" 2>/dev/null; then
    TOOLS_STATE_DIR="${HOME}/.clawden/run"
    mkdir -p "$TOOLS_STATE_DIR"
fi
ACTIVATED_TOOLS=()
TOOLS_JSON_ENTRIES=()

if [ -n "$TOOLS" ]; then
    IFS=',' read -ra TOOL_LIST <<< "$TOOLS"

    has_requested_tool() {
        local required="$1"
        for requested in "${TOOL_LIST[@]}"; do
            if [ "$(echo "$requested" | xargs)" = "$required" ]; then
                return 0
            fi
        done
        return 1
    }

    for tool in "${TOOL_LIST[@]}"; do
        tool="$(echo "$tool" | xargs)"  # trim whitespace
        setup_script="/opt/clawden/tools/${tool}/setup.sh"
        manifest="/opt/clawden/tools/${tool}/manifest.toml"

        if [ -f "$manifest" ]; then
            requires_raw="$(awk -F= '/^requires[[:space:]]*=/{print $2; exit}' "$manifest" | tr -d '[]"')"
            if [ -n "$requires_raw" ]; then
                IFS=',' read -ra REQUIRES <<< "$requires_raw"
                for dep in "${REQUIRES[@]}"; do
                    dep="$(echo "$dep" | xargs)"
                    if [ -n "$dep" ] && ! has_requested_tool "$dep"; then
                        echo "[clawden] Error: Tool '${tool}' requires '${dep}'. Add it to TOOLS." >&2
                        exit 1
                    fi
                done
            fi
        fi

        if [ -f "$setup_script" ]; then
            echo "[clawden] Setting up tool: ${tool}"
            # shellcheck disable=SC1090
            source "$setup_script"
            ACTIVATED_TOOLS+=("$tool")
            TOOLS_JSON_ENTRIES+=("    \"${tool}\": { \"version\": \"unknown\", \"bin\": \"${setup_script}\" }")
        else
            echo "[clawden] Warning: Unknown tool '${tool}', skipping" >&2
        fi
    done
fi

CLAWDEN_TOOLS="$(IFS=,; echo "${ACTIVATED_TOOLS[*]}")"
export CLAWDEN_TOOLS

{
    echo "{"
    printf '  "activated": ['
    for i in "${!ACTIVATED_TOOLS[@]}"; do
        if [ "$i" -gt 0 ]; then printf ', '; fi
        printf '"%s"' "${ACTIVATED_TOOLS[$i]}"
    done
    echo "],"
    echo '  "tools": {'
    for i in "${!TOOLS_JSON_ENTRIES[@]}"; do
        if [ "$i" -gt 0 ]; then
            echo ","
        fi
        printf '%s' "${TOOLS_JSON_ENTRIES[$i]}"
    done
    echo
    echo "  }"
    echo "}"
} > "${TOOLS_STATE_DIR}/tools.json"

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
