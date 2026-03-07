#!/usr/bin/env bash
# ClawDen Entrypoint
#
# The RUNTIME env var is pre-set by the Docker image (openclaw or zeroclaw).
# Users just need to pass LLM provider API keys via environment variables.
#
# Two launch modes:
#   CLAWDEN_USE_CLI=1  — start via `clawden run` (config translation, channel
#                        token mapping, allowed-users, pre-start validation)
#   CLAWDEN_USE_CLI=0  — (default) launch the runtime binary directly
#
# Usage:
#   docker run -e OPENAI_API_KEY=sk-... ghcr.io/codervisor/openclaw:latest
#   docker run -e ANTHROPIC_API_KEY=sk-... ghcr.io/codervisor/zeroclaw:latest
#   docker run -e CLAWDEN_USE_CLI=1 -e TELEGRAM_BOT_TOKEN=... \
#     -e CLAWDEN_CHANNELS=telegram -e CLAWDEN_ALLOWED_USERS=123 \
#     ghcr.io/codervisor/zeroclaw:latest
#   docker run ghcr.io/codervisor/openclaw:latest gateway --help

set -euo pipefail

trap 'echo "[clawden] Interrupted"; exit 130' INT TERM

RUNTIME="${RUNTIME:-}"
CLAWDEN_USE_CLI="${CLAWDEN_USE_CLI:-0}"
CLAWDEN_RUNTIMES="${HOME}/.clawden/runtimes"
CLAWDEN_MEMORY_REPO="${CLAWDEN_MEMORY_REPO:-}"
CLAWDEN_MEMORY_TOKEN="${CLAWDEN_MEMORY_TOKEN:-}"
CLAWDEN_MEMORY_PATH="${CLAWDEN_MEMORY_PATH:-}"
CLAWDEN_MEMORY_BRANCH="${CLAWDEN_MEMORY_BRANCH:-main}"

# --- Help ---
if [ "${1:-}" = "--help" ] || [ "${1:-}" = "-h" ]; then
    cat <<'EOF'
ClawDen Docker Image

Usage:
    docker run -e OPENAI_API_KEY=sk-... ghcr.io/codervisor/openclaw:latest
    docker run -e ANTHROPIC_API_KEY=sk-... ghcr.io/codervisor/zeroclaw:latest
    docker run ghcr.io/codervisor/openclaw:latest [runtime-args...]

CLI-managed mode (config translation, channel tokens, allowed-users):
    docker run -e CLAWDEN_USE_CLI=1 \
      -e TELEGRAM_BOT_TOKEN=... -e CLAWDEN_CHANNELS=telegram \
      -e CLAWDEN_ALLOWED_USERS=123456789 \
      ghcr.io/codervisor/zeroclaw:latest

Environment variables:
  OPENAI_API_KEY          OpenAI API key
  ANTHROPIC_API_KEY       Anthropic API key
  OPENROUTER_API_KEY      OpenRouter API key (access many providers)
  GEMINI_API_KEY          Google Gemini API key
  MISTRAL_API_KEY         Mistral API key
  GROQ_API_KEY            Groq API key

  TELEGRAM_BOT_TOKEN      Telegram bot token
  DISCORD_BOT_TOKEN       Discord bot token
  SLACK_BOT_TOKEN         Slack bot token
  SLACK_APP_TOKEN         Slack app-level token
  FEISHU_APP_ID           Feishu app ID
  FEISHU_APP_SECRET       Feishu app secret

  CLAWDEN_USE_CLI         Set to 1 to launch via `clawden run` (default: 0)
  CLAWDEN_CHANNELS        Comma-separated channels (e.g. telegram,discord)
  CLAWDEN_ALLOWED_USERS   Comma-separated user allowlist (e.g. Telegram IDs)
  CLAWDEN_LLM_API_KEY     LLM API key override (provider-agnostic)

  CLAWDEN_MEMORY_REPO     Git repo URL for agent memory bootstrap
  CLAWDEN_MEMORY_TOKEN    Auth token for private memory repos (e.g. GitHub PAT)
  CLAWDEN_MEMORY_PATH     Clone target (default: /home/clawden/workspace)
  CLAWDEN_MEMORY_BRANCH   Git branch to clone (default: main)

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

# ============================================================
# Memory bootstrap: restore agent workspace from a git repo.
# Delegates to `clawden workspace restore` for all git logic.
# Best-effort — failure warns but does not block runtime start.
# ============================================================
if [ -n "$CLAWDEN_MEMORY_REPO" ]; then
    RESTORE_ARGS=(--repo "$CLAWDEN_MEMORY_REPO" --branch "$CLAWDEN_MEMORY_BRANCH")
    [ -n "$CLAWDEN_MEMORY_TOKEN" ] && RESTORE_ARGS+=(--token "$CLAWDEN_MEMORY_TOKEN")
    [ -n "$CLAWDEN_MEMORY_PATH" ]  && RESTORE_ARGS+=(--target "$CLAWDEN_MEMORY_PATH")

    clawden workspace restore "${RESTORE_ARGS[@]}" || \
        echo "[clawden] Warning: workspace restore failed (continuing without memory)" >&2
fi

# ============================================================
# CLI-managed mode: delegate to `clawden run` for full config
# translation, channel token mapping, and pre-start validation.
# ============================================================
if [ "$CLAWDEN_USE_CLI" = "1" ]; then
    if ! command -v clawden >/dev/null 2>&1; then
        echo "[clawden] Error: clawden CLI not found in PATH" >&2
        exit 1
    fi

    CLI_ARGS=()

    # Channels
    IFS=',' read -ra _channels <<< "${CLAWDEN_CHANNELS:-}"
    for ch in "${_channels[@]}"; do
        ch="$(echo "$ch" | xargs)"  # trim whitespace
        [ -n "$ch" ] && CLI_ARGS+=(--channel "$ch")
    done

    # Channel token shortcuts
    [ -n "${TELEGRAM_BOT_TOKEN:-}" ] && CLI_ARGS+=(--token "$TELEGRAM_BOT_TOKEN")
    [ -n "${SLACK_APP_TOKEN:-}" ]    && CLI_ARGS+=(--app-token "$SLACK_APP_TOKEN")

    # Allowed users
    [ -n "${CLAWDEN_ALLOWED_USERS:-}" ] && CLI_ARGS+=(--allowed-users "$CLAWDEN_ALLOWED_USERS")

    # Provider / model / api-key overrides
    [ -n "${CLAWDEN_LLM_PROVIDER:-}" ] && CLI_ARGS+=(--provider "$CLAWDEN_LLM_PROVIDER")
    [ -n "${CLAWDEN_LLM_MODEL:-}" ]    && CLI_ARGS+=(--model "$CLAWDEN_LLM_MODEL")
    [ -n "${CLAWDEN_LLM_API_KEY:-}" ]  && CLI_ARGS+=(--api-key "$CLAWDEN_LLM_API_KEY")

    # System prompt
    [ -n "${CLAWDEN_SYSTEM_PROMPT:-}" ] && CLI_ARGS+=(--system-prompt "$CLAWDEN_SYSTEM_PROMPT")

    # Allow proceeding without credentials when explicitly requested
    [ "${CLAWDEN_ALLOW_MISSING_CREDENTIALS:-0}" = "1" ] && CLI_ARGS+=(--allow-missing-credentials)

    echo "[clawden] Starting ${RUNTIME} via CLI: clawden run ${CLI_ARGS[*]} ${RUNTIME} $*"
    exec clawden run "${CLI_ARGS[@]}" "$RUNTIME" "$@"
fi

# ============================================================
# Direct mode (default): launch the runtime binary directly.
# ============================================================

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
