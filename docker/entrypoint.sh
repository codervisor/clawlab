#!/usr/bin/env bash
# ClawDen Entrypoint
#
# The RUNTIME env var is pre-set by the Docker image (openclaw or zeroclaw).
# Users just need to pass LLM provider API keys via environment variables.
#
# Usage:
#   docker run -e OPENAI_API_KEY=sk-... ghcr.io/codervisor/openclaw:latest
#   docker run -e ANTHROPIC_API_KEY=sk-... ghcr.io/codervisor/zeroclaw:latest
#   docker run ghcr.io/codervisor/openclaw:latest gateway --help

set -euo pipefail

trap 'echo "[clawden] Interrupted"; exit 130' INT TERM

RUNTIME="${RUNTIME:-}"
CLAWDEN_RUNTIMES="${HOME}/.clawden/runtimes"

# --- Help ---
if [ "${1:-}" = "--help" ] || [ "${1:-}" = "-h" ]; then
    cat <<'EOF'
ClawDen Docker Image

Usage:
    docker run -e OPENAI_API_KEY=sk-... ghcr.io/codervisor/openclaw:latest
    docker run -e ANTHROPIC_API_KEY=sk-... ghcr.io/codervisor/zeroclaw:latest
    docker run ghcr.io/codervisor/openclaw:latest [runtime-args...]

Environment variables:
  OPENAI_API_KEY          OpenAI API key
  ANTHROPIC_API_KEY       Anthropic API key
  OPENROUTER_API_KEY      OpenRouter API key (access many providers)
  GEMINI_API_KEY          Google Gemini API key
  MISTRAL_API_KEY         Mistral API key
  GROQ_API_KEY            Groq API key

The container auto-detects which runtime to start from the image metadata.
Common system tools (git, curl, jq, python3, yq) are pre-installed.
EOF
    exit 0
fi

# --- Resolve runtime ---
if [ -z "$RUNTIME" ]; then
    # Allow positional override for flexibility
    if [ $# -gt 0 ] && { [ "$1" = "openclaw" ] || [ "$1" = "zeroclaw" ]; }; then
        RUNTIME="$1"
        shift
    elif [ $# -gt 0 ]; then
        echo "[clawden] Error: Unknown runtime '$1'. Use openclaw or zeroclaw." >&2
        echo "  docker run ghcr.io/codervisor/openclaw:latest" >&2
        echo "  docker run ghcr.io/codervisor/zeroclaw:latest" >&2
        exit 1
    else
        echo "[clawden] Error: RUNTIME not set. Use a runtime-specific image:" >&2
        echo "  docker run ghcr.io/codervisor/openclaw:latest" >&2
        echo "  docker run ghcr.io/codervisor/zeroclaw:latest" >&2
        exit 1
    fi
fi
export RUNTIME

# --- Resolve binary ---
LAUNCHER="${CLAWDEN_RUNTIMES}/${RUNTIME}/current/${RUNTIME}"
if [ ! -x "$LAUNCHER" ]; then
    echo "[clawden] Error: ${RUNTIME} binary not found at ${LAUNCHER}" >&2
    exit 1
fi

# --- ZeroClaw: auto-generate config if none exists ---
if [ "$RUNTIME" = "zeroclaw" ]; then
    ZEROCLAW_CONFIG_DIR="${ZEROCLAW_CONFIG_DIR:-${HOME}/.clawden/zeroclaw}"
    ZEROCLAW_CONFIG="${ZEROCLAW_CONFIG_DIR}/config.toml"
    if [ ! -f "$ZEROCLAW_CONFIG" ]; then
        mkdir -p "$ZEROCLAW_CONFIG_DIR"
        cat > "$ZEROCLAW_CONFIG" <<'TOML'
[channels_config]
cli = true

[security]
mode = "managed"
TOML
        echo "[clawden] Generated default config at ${ZEROCLAW_CONFIG}"
    fi
fi

# --- Start runtime ---
if [ $# -eq 0 ]; then
    case "$RUNTIME" in
        openclaw)
            echo "[clawden] Starting OpenClaw gateway on port 18789"
            exec "$LAUNCHER" gateway --allow-unconfigured
            ;;
        zeroclaw)
            echo "[clawden] Starting ZeroClaw daemon on port 42617"
            exec "$LAUNCHER" daemon --config-dir "$ZEROCLAW_CONFIG_DIR"
            ;;
    esac
fi

echo "[clawden] Starting ${RUNTIME}: $*"
exec "$LAUNCHER" "$@"
