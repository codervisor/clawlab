# ClawDen Docker Image
#
# Image: ghcr.io/codervisor/clawden
#
# Runtimes: OpenClaw, ZeroClaw
# Variants: basic, browser, computer
#
# Tags (build with --target):
#   :openclaw              Basic OpenClaw image (default / :latest)
#   :zeroclaw              Basic ZeroClaw image
#   :openclaw-browser      OpenClaw + Chromium/Playwright
#   :zeroclaw-browser      ZeroClaw + Chromium/Playwright
#   :openclaw-computer     OpenClaw + browser + desktop (Xvfb/VNC/noVNC)
#   :zeroclaw-computer     ZeroClaw + browser + desktop (Xvfb/VNC/noVNC)
#
# Quick start:
#   docker run -e OPENAI_API_KEY=sk-... ghcr.io/codervisor/clawden:openclaw
#   docker run -e ANTHROPIC_API_KEY=sk-... ghcr.io/codervisor/clawden:zeroclaw
#
# Runtimes are installed using `clawden install`, the same code path
# used for local development. This ensures Docker and CLI stay in sync.

# Pinned runtime versions — bump these to upgrade
ARG OPENCLAW_VERSION=2026.3.2
ARG ZEROCLAW_VERSION=0.1.7

# --- Stage 1: Build the clawden CLI ---
FROM rust:1-bookworm AS builder

WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY crates/ crates/
RUN cargo build -p clawden-cli --release

# --- Stage 2: Base runtime image ---
FROM debian:trixie-slim AS base

# Node.js 22 LTS — copied from official image (no curl|bash)
COPY --from=node:22-bookworm-slim /usr/local/bin/node /usr/local/bin/node
COPY --from=node:22-bookworm-slim /usr/local/lib/node_modules /usr/local/lib/node_modules
COPY --from=mikefarah/yq:4.47.2 /usr/bin/yq /usr/local/bin/yq
RUN ln -s /usr/local/lib/node_modules/npm/bin/npm-cli.js /usr/local/bin/npm \
    && ln -s /usr/local/lib/node_modules/corepack/dist/corepack.js /usr/local/bin/corepack \
    && corepack enable pnpm

# System tools — all pre-installed, no activation needed
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    curl \
    wget \
    git \
    openssh-client \
    jq \
    tree \
    file \
    gzip \
    unzip \
    zip \
    python3 \
    python3-pip \
    python3-venv \
    ripgrep \
    fd-find \
    bat \
    sqlite3 \
    netcat-openbsd \
    socat \
    dnsutils \
    traceroute \
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/*

# Non-root user (UID 10000 — compatible with most cloud platforms)
RUN groupadd -g 10000 clawden \
    && useradd -u 10000 -g clawden -m -s /bin/bash clawden

# Directory structure
RUN mkdir -p /home/clawden/workspace /etc/clawden \
    && chown -R clawden:clawden /home/clawden

# ClawDen CLI from builder stage
COPY --from=builder /src/target/release/clawden /usr/local/bin/clawden

# Entrypoint and defaults
COPY docker/entrypoint.sh /opt/clawden/entrypoint.sh
COPY docker/defaults.toml /etc/clawden/defaults.toml
RUN chmod +x /opt/clawden/entrypoint.sh

# --- Install runtimes ---
USER clawden

ARG OPENCLAW_VERSION
ARG ZEROCLAW_VERSION
RUN clawden install "openclaw@${OPENCLAW_VERSION}" \
    && clawden install "zeroclaw@${ZEROCLAW_VERSION}" \
    && for runtime in openclaw zeroclaw; do \
         bin="/home/clawden/.clawden/runtimes/${runtime}/current/${runtime}"; \
         case "$runtime" in \
           zeroclaw) probe="--version" ;; \
           openclaw) probe="--help" ;; \
         esac; \
         echo "[clawden] Smoke testing ${runtime} (${probe})"; \
         "$bin" "$probe" >/tmp/${runtime}.probe.log 2>&1 \
           || { cat /tmp/${runtime}.probe.log >&2; exit 1; }; \
         rm -f /tmp/${runtime}.probe.log; \
       done \
    && rm -rf /home/clawden/.clawden/cache

USER root

WORKDIR /home/clawden/workspace
VOLUME /home/clawden/workspace

USER clawden
ENTRYPOINT ["/opt/clawden/entrypoint.sh"]

# --- OpenClaw basic (default) ---
FROM base AS openclaw
ENV RUNTIME=openclaw
EXPOSE 18789
HEALTHCHECK --interval=30s --timeout=5s --retries=3 \
    CMD curl -sf http://localhost:18789/health || exit 1

# --- ZeroClaw basic ---
FROM base AS zeroclaw
ENV RUNTIME=zeroclaw
EXPOSE 42617
HEALTHCHECK --interval=30s --timeout=5s --retries=3 \
    CMD curl -sf http://localhost:42617/health || exit 1

# --- Browser base (shared layer: Chromium + Playwright) ---
FROM base AS base-browser
USER root
RUN apt-get update && apt-get install -y --no-install-recommends \
    chromium \
    && PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD=1 npm install -g playwright \
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/* /root/.npm /tmp/*
USER clawden

FROM base-browser AS openclaw-browser
ENV RUNTIME=openclaw
EXPOSE 18789
HEALTHCHECK --interval=30s --timeout=5s --retries=3 \
    CMD curl -sf http://localhost:18789/health || exit 1

FROM base-browser AS zeroclaw-browser
ENV RUNTIME=zeroclaw
EXPOSE 42617
HEALTHCHECK --interval=30s --timeout=5s --retries=3 \
    CMD curl -sf http://localhost:42617/health || exit 1

# --- Computer base (shared layer: browser + Xvfb/VNC/noVNC desktop) ---
FROM base-browser AS base-computer
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

FROM base-computer AS openclaw-computer
ENV RUNTIME=openclaw
EXPOSE 18789 6080
HEALTHCHECK --interval=30s --timeout=5s --retries=3 \
    CMD curl -sf http://localhost:18789/health || exit 1

FROM base-computer AS zeroclaw-computer
ENV RUNTIME=zeroclaw
EXPOSE 42617 6080
HEALTHCHECK --interval=30s --timeout=5s --retries=3 \
    CMD curl -sf http://localhost:42617/health || exit 1

# Default: openclaw basic
FROM openclaw AS latest
