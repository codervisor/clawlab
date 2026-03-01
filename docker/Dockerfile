# ClawDen Runtime Container
# Single generic image supporting all Phase 1 runtimes:
# - OpenClaw (Node.js, JSON5 config, gateway on port 18789)
# - NanoClaw (Node.js, code-driven, Claude Agent SDK)
# - ZeroClaw (Rust binary, TOML + env vars)
# - PicoClaw (Go binary, JSON config)
# - OpenFang (Rust binary, Agent OS, TOML config, dashboard on port 4200)
#
# Image: ghcr.io/codervisor/clawden-runtime:latest
# Base: Debian slim + Node.js 22 LTS (for OpenClaw and NanoClaw)
#
# Runtimes are pulled from their upstream GitHub releases / npm:
#   ZeroClaw: https://github.com/zeroclaw-labs/zeroclaw/releases
#   PicoClaw: https://github.com/picoclaw-labs/picoclaw/releases
#   OpenClaw: npm install -g openclaw
#   NanoClaw: https://github.com/qwibitai/nanoclaw (git clone)
#   OpenFang: https://github.com/RightNow-AI/openfang/releases

# Pinned runtime versions — bump these to upgrade
ARG ZEROCLAW_VERSION=0.1.7
ARG PICOCLAW_VERSION=latest
ARG OPENCLAW_VERSION=latest
ARG OPENFANG_VERSION=0.2.0

FROM debian:bookworm-slim AS base

# Install system dependencies + Node.js 22 LTS
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    curl \
    dnsutils \
    git \
    && curl -fsSL https://deb.nodesource.com/setup_22.x | bash - \
    && apt-get install -y --no-install-recommends nodejs \
    && npm install -g pnpm \
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/*

# Create non-root clawden user (UID 10000)
RUN groupadd -g 10000 clawden && \
    useradd -u 10000 -g clawden -m -s /bin/bash clawden

# Create directory structure
RUN mkdir -p /opt/clawden/runtimes/openclaw \
             /opt/clawden/runtimes/nanoclaw \
             /opt/clawden/tools/git \
             /opt/clawden/tools/http \
             /opt/clawden/tools/browser \
             /opt/clawden/tools/gui \
             /home/clawden/workspace \
    && chown -R clawden:clawden /opt/clawden /home/clawden

# Copy entrypoint and tool setup scripts
COPY docker/entrypoint.sh /opt/clawden/entrypoint.sh
COPY docker/tools/ /opt/clawden/tools/
RUN chmod +x /opt/clawden/entrypoint.sh \
    && find /opt/clawden/tools -name "setup.sh" -exec chmod +x {} \;

# Copy internal defaults
COPY docker/defaults.toml /etc/clawden/defaults.toml

# --- Phase 1 runtimes: download pre-built releases ---

# ZeroClaw (Rust binary) — from GitHub Releases
ARG ZEROCLAW_VERSION
ARG TARGETARCH
RUN set -eux; \
    case "${TARGETARCH}" in \
        amd64)  ZC_ARCH="x86_64-unknown-linux-gnu" ;; \
        arm64)  ZC_ARCH="aarch64-unknown-linux-gnu" ;; \
        *)      echo "Unsupported arch: ${TARGETARCH}" >&2; exit 1 ;; \
    esac; \
    curl -fsSL "https://github.com/zeroclaw-labs/zeroclaw/releases/download/v${ZEROCLAW_VERSION}/zeroclaw-${ZC_ARCH}.tar.gz" \
        -o /tmp/zeroclaw.tar.gz \
    && tar -xzf /tmp/zeroclaw.tar.gz -C /tmp \
    && mv /tmp/zeroclaw /opt/clawden/runtimes/zeroclaw \
    && chmod +x /opt/clawden/runtimes/zeroclaw \
    && rm -rf /tmp/zeroclaw*

# PicoClaw (Go binary) — from GitHub Releases
ARG PICOCLAW_VERSION
RUN set -eux; \
    curl -fsSL "https://github.com/picoclaw-labs/picoclaw/releases/download/picoclaw/picoclaw_x64.7z" \
        -o /tmp/picoclaw.7z \
    && apt-get update && apt-get install -y --no-install-recommends p7zip-full \
    && 7z x /tmp/picoclaw.7z -o/tmp/picoclaw \
    && find /tmp/picoclaw -type f -name 'picoclaw*' -exec mv {} /opt/clawden/runtimes/picoclaw \; \
    && chmod +x /opt/clawden/runtimes/picoclaw \
    && apt-get purge -y p7zip-full && apt-get autoremove -y \
    && apt-get clean && rm -rf /var/lib/apt/lists/* /tmp/picoclaw*

# OpenClaw (Node.js app) — from npm
ARG OPENCLAW_VERSION
RUN npm install -g "openclaw@${OPENCLAW_VERSION}" \
    && ln -s "$(npm root -g)/openclaw" /opt/clawden/runtimes/openclaw

# NanoClaw (Node.js app) — cloned from GitHub (no npm/releases yet)
ARG NANOCLAW_REF=main
RUN set -eux; \
    git clone --depth 1 --branch "${NANOCLAW_REF}" \
       https://github.com/qwibitai/nanoclaw.git /opt/clawden/runtimes/nanoclaw \
    && cd /opt/clawden/runtimes/nanoclaw \
    && npm install --production \
    && rm -rf .git .github docs assets

# OpenFang (Rust binary) — from GitHub Releases (.deb package)
# Note: arm64 .deb not yet available upstream; amd64 only for now
ARG OPENFANG_VERSION
ARG TARGETARCH
RUN set -eux; \
    case "${TARGETARCH}" in \
        amd64) \
            curl -fsSL "https://github.com/RightNow-AI/openfang/releases/download/v${OPENFANG_VERSION}/OpenFang_0.1.0_amd64.deb" \
                -o /tmp/openfang.deb \
            && dpkg -x /tmp/openfang.deb /tmp/openfang-extracted \
            && find /tmp/openfang-extracted -type f -name 'openfang' -exec mv {} /opt/clawden/runtimes/openfang \; \
            && chmod +x /opt/clawden/runtimes/openfang \
            && rm -rf /tmp/openfang* \
            ;; \
        arm64) \
            echo "[clawden] Warning: OpenFang arm64 binary not yet available upstream, skipping" >&2 \
            ;; \
        *)  echo "Unsupported arch: ${TARGETARCH}" >&2; exit 1 ;; \
    esac

WORKDIR /home/clawden/workspace
VOLUME /home/clawden/workspace

# Health check — ClawDen or the runtime exposes GET /health
HEALTHCHECK --interval=30s --timeout=5s --retries=3 \
    CMD curl -f http://localhost:${RUNTIME_PORT:-8080}/health || exit 1

USER clawden

ENTRYPOINT ["/opt/clawden/entrypoint.sh"]
