#!/usr/bin/env bash
# Tool setup: core-utils
# Ensures common utility binaries are available.

if command -v jq &>/dev/null; then
    echo "[clawden/tools/core-utils] core utils ready"
else
    echo "[clawden/tools/core-utils] Warning: jq not found in image" >&2
fi
