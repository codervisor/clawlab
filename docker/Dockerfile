# ClawDen Runtime Container
# Multi-target image supporting runtime variants:
# - OpenClaw (Node.js, JSON5 config, gateway on port 18789)
# - NanoClaw (Node.js, code-driven, Claude Agent SDK)
# - ZeroClaw (Rust binary, TOML + env vars)
# - PicoClaw (Go binary, JSON config)
# - OpenFang (Rust binary, Agent OS, TOML config, dashboard on port 4200)
#
# Targets:
#   latest        -> core + standard tools (default)
#   browser       -> latest + Chromium + Playwright CLI
#   computer      -> browser + Xvfb/VNC/noVNC/fluxbox
#   full          -> computer + compiler toolchain
# Base: Debian slim + Node.js 22 LTS (for OpenClaw and NanoClaw)
#
# Runtimes are installed using `clawden-cli install`, the same code path
# used for local development. This ensures Docker and CLI stay in sync.

# Pinned runtime versions — bump these to upgrade
ARG ZEROCLAW_VERSION=0.1.7
ARG PICOCLAW_VERSION=latest
ARG OPENCLAW_VERSION=latest
ARG OPENFANG_VERSION=0.3.4

# --- Stage 1: Build the clawden CLI ---
FROM rust:1-bookworm AS builder

WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY crates/ crates/
RUN cargo build -p clawden-cli --release

# --- Stage 2: Runtime image ---
FROM debian:bookworm-slim AS base

# Install system dependencies + Node.js 22 LTS
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    curl \
    dnsutils \
    git \
    jq \
    tree \
    unzip \
    zip \
    p7zip-full \
    && curl -fsSL https://deb.nodesource.com/setup_22.x | bash - \
    && apt-get install -y --no-install-recommends nodejs \
    && npm install -g pnpm \
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/*

# Create non-root clawden user (UID 10000)
RUN groupadd -g 10000 clawden && \
    useradd -u 10000 -g clawden -m -s /bin/bash clawden

# Create directory structure
RUN mkdir -p /opt/clawden/tools/git \
             /opt/clawden/tools/core-utils \
             /opt/clawden/tools/http \
             /opt/clawden/tools/python \
             /opt/clawden/tools/code-tools \
             /opt/clawden/tools/database \
             /opt/clawden/tools/network \
             /opt/clawden/tools/sandbox \
             /opt/clawden/tools/browser \
             /opt/clawden/tools/gui \
             /home/clawden/workspace \
    && chown -R clawden:clawden /opt/clawden /home/clawden

# Standard tool layer (spec 024 phase 2)
RUN apt-get update && apt-get install -y --no-install-recommends \
    python3 \
    python3-pip \
    python3-venv \
    ripgrep \
    fd-find \
    bat \
    sqlite3 \
    netcat-openbsd \
    socat \
    traceroute \
    bubblewrap \
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/*

# Copy entrypoint and tool setup scripts
COPY docker/entrypoint.sh /opt/clawden/entrypoint.sh
COPY docker/tools/ /opt/clawden/tools/
RUN chmod +x /opt/clawden/entrypoint.sh \
    && find /opt/clawden/tools -name "setup.sh" -exec chmod +x {} \;

# Copy internal defaults
COPY docker/defaults.toml /etc/clawden/defaults.toml

# Copy clawden CLI from builder stage
COPY --from=builder /src/target/release/clawden-cli /usr/local/bin/clawden-cli

# --- Install runtimes using clawden-cli (same code path as local dev) ---
# Run as clawden user so runtimes install to /home/clawden/.clawden/
USER clawden

ARG ZEROCLAW_VERSION
ARG OPENCLAW_VERSION
ARG PICOCLAW_VERSION
ARG NANOCLAW_REF=main
ARG OPENFANG_VERSION
RUN clawden-cli install "zeroclaw@${ZEROCLAW_VERSION}" \
    && clawden-cli install "openclaw@${OPENCLAW_VERSION}" \
    && clawden-cli install "picoclaw@${PICOCLAW_VERSION}" \
    && clawden-cli install nanoclaw \
    && clawden-cli install "openfang@${OPENFANG_VERSION}" \
    && for runtime in zeroclaw picoclaw openclaw nanoclaw openfang; do \
         bin="/home/clawden/.clawden/runtimes/${runtime}/current/${runtime}"; \
         case "$runtime" in \
           zeroclaw|openfang) probe="--version" ;; \
           picoclaw|openclaw|nanoclaw) probe="--help" ;; \
           *) probe="--help" ;; \
         esac; \
         echo "[clawden] Smoke testing ${runtime} (${probe})"; \
         "$bin" "$probe" >/tmp/${runtime}.probe.log 2>&1 \
           || { cat /tmp/${runtime}.probe.log >&2; exit 1; }; \
         rm -f /tmp/${runtime}.probe.log; \
       done \
    && rm -rf /home/clawden/.clawden/cache

# Switch back to root to clean up build-only deps, then back to clawden
USER root
RUN apt-get update \
    && apt-get purge -y p7zip-full && apt-get autoremove -y \
    && apt-get clean && rm -rf /var/lib/apt/lists/*

WORKDIR /home/clawden/workspace
VOLUME /home/clawden/workspace

# Health check — ClawDen or the runtime exposes GET /health
HEALTHCHECK --interval=30s --timeout=5s --retries=3 \
    CMD curl -f http://localhost:${RUNTIME_PORT:-8080}/health || exit 1

USER clawden

ENTRYPOINT ["/opt/clawden/entrypoint.sh"]

# --- Variant stage: browser ---
FROM base AS browser

USER root
RUN apt-get update && apt-get install -y --no-install-recommends \
    chromium \
    && PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD=1 npm install -g playwright \
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/* /root/.npm /tmp/*

USER clawden

# --- Variant stage: computer ---
FROM browser AS computer

USER root
RUN apt-get update && apt-get install -y --no-install-recommends \
    xvfb \
    x11vnc \
    novnc \
    websockify \
    fluxbox \
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/*

USER clawden

# --- Variant stage: full ---
FROM computer AS full

USER root
RUN apt-get update && apt-get install -y --no-install-recommends \
    gcc \
    g++ \
    make \
    cmake \
    pkg-config \
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/*

USER clawden

# Keep latest as the default build output when no --target is provided.
FROM base AS latest
