# Upstream Runtime Sources & Research

Before adding or updating any adapter, research the runtime's upstream codebase to
ensure metadata (channels, capabilities, config format, default port) is accurate.

## Source Registry

| Runtime | Source Type | Location | Version Query |
|---------|-----------|----------|---------------|
| **ZeroClaw** | GitHub releases | [`zeroclaw-labs/zeroclaw`](https://github.com/zeroclaw-labs/zeroclaw) | GitHub Release API (`/releases/latest`) |
| **PicoClaw** | GitHub releases | [`picoclaw-labs/picoclaw`](https://github.com/picoclaw-labs/picoclaw) | GitHub Release API |
| **OpenClaw** | npm package | [`openclaw`](https://www.npmjs.com/package/openclaw) | `npm view openclaw version --json` |
| **NanoClaw** | Git repo | [`qwibitai/nanoclaw`](https://github.com/qwibitai/nanoclaw) | `git ls-remote` HEAD branch |
| **OpenFang** | TBD | Referenced in specs/docker but no install logic | Not yet integrated |
| **IronClaw** | TBD | Declared in `ClawRuntime` enum, no adapter | Not yet integrated |
| **NullClaw** | TBD | Declared in `ClawRuntime` enum, has start_args | Not yet integrated |
| **MicroClaw** | TBD | Declared in `ClawRuntime` enum only | Not yet integrated |
| **MimiClaw** | TBD | Declared in `ClawRuntime` enum only | Not yet integrated |

## Research Workflow

Run this workflow before creating or updating any adapter.

### Step 1: Check upstream README/docs for metadata

For each runtime repo, look for:

| What to find | Where to look | Maps to adapter field |
|-------------|---------------|----------------------|
| Supported messaging channels | README "Channels" / "Integrations" section | `channel_support` HashMap |
| Config file format | README "Configuration" section, example configs | `config_format` field |
| Default listening port | README, CLI `--help`, Dockerfile `EXPOSE` | `default_port` field |
| Core capabilities | README feature list, CLI subcommands | `capabilities` vec |
| Implementation language | repo language stats, package.json/Cargo.toml | `language` field |

### Step 2: Check upstream CHANGELOG / releases

Look at recent releases for:
- **New channels added** → add to `channel_support` with `ChannelSupport::Native` or `::Via("mechanism")`
- **Channels removed** → change to `ChannelSupport::Unsupported`
- **Config format changes** → update `config_format`
- **New capabilities** → add to `capabilities` vec
- **Port changes** → update `default_port`
- **Breaking CLI changes** → update `runtime_start_args()` and `runtime_supported_extra_args()` in `install.rs`

### Step 3: Check channel support details

For each channel the runtime claims to support, determine the support level:

```rust
// Native built-in support
ChannelSupport::Native

// Supported via a mechanism (specify which)
ChannelSupport::Via("Baileys".into())        // e.g., WhatsApp via Baileys lib
ChannelSupport::Via("skill".into())          // e.g., via runtime's skill/plugin system
ChannelSupport::Via("signal-cli".into())     // e.g., via external bridge
ChannelSupport::Via("Meta Cloud API".into()) // e.g., via official API

// Not supported — ClawDen will proxy if needed
ChannelSupport::Unsupported
```

### Step 4: Verify install method

Check how the runtime should be installed for direct mode (`crates/clawden-core/src/install.rs`):

| Source type | Install method | Prerequisites |
|------------|---------------|---------------|
| GitHub releases (binary) | Download tar.gz/7z, extract binary | `curl`, possibly `7z` |
| npm package | `npm install -g --prefix` | `node`, `npm` |
| Git repo + build | `git clone` + `pnpm install` | `git`, `node`, `pnpm` |
| Cargo crate | `cargo install` | `rustup`, `cargo` |
| WASM module | Download .wasm artifact | WASM runtime |

### Step 5: Document findings

After research, update these adapter fields if they've changed:

```rust
RuntimeMetadata {
    runtime: ClawRuntime::{Variant},
    version: "unknown".to_string(),  // still "unknown" — resolved at query time
    language: "{verified_language}".to_string(),
    capabilities: vec![/* verified capabilities */],
    default_port: Some({verified_port}),  // or None
    config_format: Some("{verified_format}".to_string()),
    channel_support,  // verified channel HashMap
}
```

## Quick Research Commands

```bash
# Check latest ZeroClaw release
curl -s https://api.github.com/repos/zeroclaw-labs/zeroclaw/releases/latest | jq '.tag_name, .body'

# Check latest PicoClaw release
curl -s https://api.github.com/repos/picoclaw-labs/picoclaw/releases/latest | jq '.tag_name, .body'

# Check latest OpenClaw version + README
npm view openclaw version
npm view openclaw readme | head -100

# Check NanoClaw latest commit
git ls-remote https://github.com/qwibitai/nanoclaw.git HEAD

# Check a runtime's README for channel list (example: ZeroClaw)
curl -s https://api.github.com/repos/zeroclaw-labs/zeroclaw/readme | jq -r '.content' | base64 -d | head -200
```

## Cross-Reference: Install Logic

The install logic lives in `crates/clawden-core/src/install.rs`. Key functions per runtime:

| Runtime | Installer function | Source pattern |
|---------|-------------------|----------------|
| ZeroClaw | `install_zeroclaw()` | `github_release_assets("zeroclaw-labs", "zeroclaw", version)` → tar.gz |
| PicoClaw | `install_picoclaw()` | Direct URL: `picoclaw-labs/picoclaw/releases/download/picoclaw/picoclaw_x64.7z` |
| OpenClaw | `install_openclaw()` | `npm install -g --prefix` |
| NanoClaw | `install_nanoclaw()` | `git clone --depth 1 https://github.com/qwibitai/nanoclaw.git` + `pnpm install` |

When adding a new runtime, also add a corresponding `install_{slug}()` method in `install.rs`
and update `ensure_runtime_supported()` to include the new slug.
