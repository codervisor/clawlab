---
status: planned
created: 2026-03-05
priority: high
tags:
- refactor
- runtime
- adapter
- architecture
- install
- config
depends_on:
- 010-claw-runtime-interface
created_at: 2026-03-05T04:02:12.994673360Z
updated_at: 2026-03-05T04:02:12.994673360Z
---

# Runtime Descriptor Refactor — Decouple Per-Runtime Metadata from Hardcoded Match Statements

## Overview

Per-runtime behavior is scattered across **11+ match statements** in `install.rs`, `process.rs`, and `config_gen.rs`. Adding a new runtime (e.g., IronClaw) requires coordinated edits to all these files simultaneously. The `ClawAdapter` trait cleanly abstracts lifecycle operations, but **installation, config generation, health checks, CLI arguments, and version resolution** are all hardcoded outside the trait.

This spec consolidates all per-runtime metadata into a single `RuntimeDescriptor` trait (or struct) so that adding a new runtime is a single-file change — implement the descriptor, and every subsystem picks it up automatically.

### Current Coupling Hotspots

| File | Coupling | Match Count |
|------|----------|-------------|
| `crates/clawden-core/src/install.rs` | install dispatch, version query, start args, subcommand hints, config-dir support, supported runtimes list | 7 |
| `crates/clawden-core/src/process.rs` | health check URLs/ports per runtime | 1 |
| `crates/clawden-cli/src/commands/config_gen.rs` | config format (TOML/JSON), config injection flags, onboard command support | 3 |

**Risk:** Each new runtime requires ~6–8 file edits across 3 crates with no compile-time guarantee of completeness.

### Why Now

- Spec 040 (smoke test matrix) revealed 5 blockers caused by runtime-specific hardcoding inconsistencies
- IronClaw, NullClaw, MicroClaw, MimiClaw are defined in the enum but have incomplete integration
- Community runtime contributions are blocked by the complexity of touching 8+ files

## Design

### 1. `RuntimeDescriptor` Trait

Extend or complement `ClawAdapter` with a new trait in `clawden-core` that captures all the metadata currently scattered in match statements:

```rust
/// Describes a runtime's installation, configuration, and CLI behavior.
/// Implement this once per runtime — all subsystems consume it.
pub trait RuntimeDescriptor: Send + Sync {
    // === Identity ===
    fn slug(&self) -> &'static str;
    fn runtime(&self) -> ClawRuntime;
    fn metadata(&self) -> RuntimeMetadata; // existing, from ClawAdapter

    // === Installation ===
    fn install_source(&self) -> InstallSource;
    fn is_direct_install_supported(&self) -> bool { true }

    // === Version Resolution ===
    fn query_latest_version(&self) -> Result<String>;

    // === CLI Behavior ===
    fn default_start_args(&self) -> &'static [&'static str] { &[] }
    fn subcommand_hints(&self) -> &'static [(&'static str, &'static str)] { &[] }
    fn supported_extra_args(&self) -> &'static [&'static str] { &[] }

    // === Health ===
    fn health_url(&self) -> Option<String> {
        self.metadata().default_port.map(|p|
            format!("http://127.0.0.1:{p}/health")
        )
    }

    // === Configuration ===
    fn config_format(&self) -> ConfigFormat { ConfigFormat::None }
    fn supports_config_dir(&self) -> bool { false }
    fn config_dir_flag(&self) -> ConfigDirFlag { ConfigDirFlag::ConfigDir }
    fn has_onboard_command(&self) -> bool { false }

    // === Cost ===
    fn cost_tier(&self) -> u8 { 2 }
}

pub enum InstallSource {
    GithubRelease { owner: &'static str, repo: &'static str },
    Npm { package: &'static str },
    GitClone { url: &'static str },
    NotAvailable,
}

pub enum ConfigFormat {
    Toml,
    Json,
    EnvVars,
    None,
}

pub enum ConfigDirFlag {
    ConfigDir,                        // --config-dir <dir>
    ConfigFile { filename: &'static str }, // --config <dir/filename>
}
```

### 2. Per-Runtime Descriptor Implementations

Each runtime gets a struct implementing `RuntimeDescriptor`. Example:

```rust
pub struct ZeroClawDescriptor;

impl RuntimeDescriptor for ZeroClawDescriptor {
    fn slug(&self) -> &'static str { "zeroclaw" }
    fn runtime(&self) -> ClawRuntime { ClawRuntime::ZeroClaw }
    fn install_source(&self) -> InstallSource {
        InstallSource::GithubRelease { owner: "zeroclaw-labs", repo: "zeroclaw" }
    }
    fn default_start_args(&self) -> &'static [&'static str] { &["daemon"] }
    fn config_format(&self) -> ConfigFormat { ConfigFormat::Toml }
    fn supports_config_dir(&self) -> bool { true }
    fn has_onboard_command(&self) -> bool { true }
    fn cost_tier(&self) -> u8 { 2 }
    fn health_url(&self) -> Option<String> {
        Some("http://127.0.0.1:42617/health".into())
    }
    fn query_latest_version(&self) -> Result<String> {
        Ok(normalize_version(
            &github_release_assets("zeroclaw-labs", "zeroclaw", "latest")?.tag
        ))
    }
    // ...
}
```

### 3. Descriptor Registry

Add a `DescriptorRegistry` (or extend `AdapterRegistry`) that holds `HashMap<ClawRuntime, Arc<dyn RuntimeDescriptor>>`. All consumers query the registry instead of match-hardcoding:

```rust
// Before (install.rs):
match runtime {
    "zeroclaw" => self.install_zeroclaw(...),
    "picoclaw" => self.install_picoclaw(...),
    ...
}

// After:
let desc = registry.descriptor(runtime)?;
match desc.install_source() {
    InstallSource::GithubRelease { owner, repo } => {
        self.install_github_release(owner, repo, &version, &tmp_dir)?
    }
    InstallSource::Npm { package } => {
        self.install_npm(package, &version)?
    }
    InstallSource::GitClone { url } => {
        self.install_git_clone(url, &version, &tmp_dir)?
    }
    ...
}
```

### 4. Unify or Compose with ClawAdapter

Two options (decide during implementation):

- **Option A — Supertrait**: `trait ClawAdapter: RuntimeDescriptor` — every adapter also provides descriptor metadata. Simplest, but forces adapters to carry install logic.
- **Option B — Separate registries**: `AdapterRegistry` for lifecycle ops, `DescriptorRegistry` for metadata. Allows descriptors for runtimes without adapters (e.g., runtimes that only support direct mode).

**Recommendation:** Option B — some runtimes (NullClaw, IronClaw) may only have descriptors initially without full adapter implementations.

### 5. Refactor Targets

| Current Location | Refactored To |
|-----------------|---------------|
| `install.rs::install_runtime()` match | Generic install by `InstallSource` variant |
| `install.rs::query_latest_version()` match | `descriptor.query_latest_version()` |
| `install.rs::runtime_default_start_args()` | `descriptor.default_start_args()` |
| `install.rs::runtime_subcommand_hints()` | `descriptor.subcommand_hints()` |
| `install.rs::runtime_supports_config_dir()` | `descriptor.supports_config_dir()` |
| `install.rs::ensure_runtime_supported()` | `descriptor.is_direct_install_supported()` |
| `process.rs::runtime_health_url()` | `descriptor.health_url()` |
| `config_gen.rs::generate_config_dir()` | Dispatch on `descriptor.config_format()` |
| `config_gen.rs::inject_config_dir_arg()` | Dispatch on `descriptor.config_dir_flag()` |
| `config_gen.rs::has_onboard_command()` | `descriptor.has_onboard_command()` |
| `manager.rs::runtime_cost_tier()` | `descriptor.cost_tier()` |

## Plan

- [ ] Define `RuntimeDescriptor` trait and supporting enums in `clawden-core`
- [ ] Implement `RuntimeDescriptor` for all 5 supported runtimes (ZeroClaw, OpenClaw, PicoClaw, NanoClaw, OpenFang)
- [ ] Add stub descriptors for enum-only runtimes (IronClaw, NullClaw, MicroClaw, MimiClaw) with `is_direct_install_supported() = false`
- [ ] Create `DescriptorRegistry` with lookup by slug and `ClawRuntime` enum
- [ ] Refactor `install.rs` — replace all match statements with descriptor calls
- [ ] Refactor `process.rs` — replace `runtime_health_url()` with descriptor
- [ ] Refactor `config_gen.rs` — replace format/flag/onboard matches with descriptor
- [ ] Refactor `manager.rs` — replace `runtime_cost_tier()` with descriptor
- [ ] Delete dead per-runtime private methods in `install.rs` (consolidate into generic helpers)
- [ ] Update tests to use descriptor-based API

## Test

- [ ] `cargo test -p clawden-core` passes — descriptor trait + registry work
- [ ] `cargo test -p clawden-cli` passes — all CLI integration tests pass with refactored code
- [ ] `cargo clippy` clean across workspace
- [ ] Adding a hypothetical new runtime requires only one new descriptor file (verify manually)
- [ ] All existing `clawden run <runtime>` behaviors are preserved (no regression)
- [ ] Feature-gated compilation still works (`cargo build --no-default-features`)

## Notes

- The `ClawRuntime` enum itself stays — it's useful for type-safe dispatch. The goal is eliminating the *scattered match statements*, not the enum.
- Channel-specific config generation (telegram token fields, discord fields, etc.) is channel coupling, not runtime coupling — out of scope for this spec.
- Docker entrypoint.sh and Dockerfile are shell scripts with their own match/case statements — these stay as-is since they run outside Rust.
- This refactor is backwards-compatible: the public API surface (`runtime_default_start_args()`, etc.) can be kept as thin wrappers over descriptor calls during migration.