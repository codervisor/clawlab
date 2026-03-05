---
status: in-progress
created: 2026-03-05
priority: high
tags:
- refactor
- architecture
- adapters
- error-handling
- deduplication
- code-quality
depends_on:
- 041-runtime-descriptor-refactor
created_at: 2026-03-05T07:50:32.160972724Z
updated_at: 2026-03-05T08:19:13.764245770Z
transitions:
- status: in-progress
  at: 2026-03-05T08:19:13.764245770Z
---
# Rust Codebase Structural Refactor — Deduplication, Error Types & Adapter Generics

## Overview

A full audit of the Rust codebase (~7,000+ lines across 5 crates) revealed **6 systemic structural problems** and **40+ code smells**. Spec 041 successfully consolidated per-runtime metadata into `RuntimeDescriptor`, but the remaining issues fall into different categories: massive adapter copy-paste, hardcoded provider/channel lists, god functions, stringly-typed errors, duplicated TOML/JSON config logic, and global mutable state.

These problems compound when adding new runtimes, providers, or channels — every addition requires editing 5–8 files with no compile-time safety net.

### Audit Summary

| Category | Hotspots | Impact |
|----------|----------|--------|
| Adapter copy-paste | 5 adapter files, ~70% identical code | New adapter = copy 500 lines of boilerplate |
| Hardcoded lists | Provider/channel constants in 8+ files | New provider/channel = 8 match edits |
| God functions | `exec_run` (336 lines), `exec_up` (260 lines), `validate_direct_runtime_config` (230 lines) | Untestable; impossible to reason about |
| Stringly-typed errors | `Result<T, String>` throughout manager.rs | No error classification or chaining |
| TOML/JSON duplication | `inject_proxy_config` / `inject_proxy_config_json` 95% identical | Bug fix must be applied twice |
| Global mutable state | 5 separate `OnceLock<Mutex<HashMap>>` stores | Test isolation impossible without hacks |

### Why Now

- Spec 041 proved the descriptor pattern works; this extends it to providers and channels
- Community adapter contributions are blocked by the 500-line copy-paste requirement
- CLI command functions are untestable monoliths, making regression bugs likely
- Provider/channel lists are already diverging between files (inconsistent coverage)

## Design

### Phase 1: Generic `DockerAdapter<R>` (clawden-adapters)

All 5 adapters (openclaw, zeroclaw, picoclaw, openfang, nanoclaw) share ~70% identical code. Extract into a generic adapter parameterized by a metadata trait:

```rust
// crates/clawden-adapters/src/docker_adapter.rs
pub trait RuntimeMeta: Send + Sync + 'static {
    const RUNTIME: ClawRuntime;
    fn metadata() -> RuntimeMetadata;
}

pub struct DockerAdapter<R: RuntimeMeta> {
    store: &'static Mutex<HashMap<String, RuntimeConfig>>,
    _marker: PhantomData<R>,
}

#[async_trait]
impl<R: RuntimeMeta> ClawAdapter for DockerAdapter<R> {
    fn metadata(&self) -> RuntimeMetadata { R::metadata() }
    async fn start(&self, config: &AgentConfig) -> Result<AgentHandle, anyhow::Error> { /* shared */ }
    async fn stop(&self, handle: &AgentHandle) -> Result<(), anyhow::Error> { /* shared */ }
    // ... all shared methods written once
}
```

Each runtime becomes a ~20-line metadata-only file:

```rust
pub struct OpenClawMeta;
impl RuntimeMeta for OpenClawMeta {
    const RUNTIME: ClawRuntime = ClawRuntime::OpenClaw;
    fn metadata() -> RuntimeMetadata { /* channel map + capabilities */ }
}
pub type OpenClawAdapter = DockerAdapter<OpenClawMeta>;
```

Also: replace 5 separate `OnceLock<Mutex<HashMap>>` config stores with a single shared `ConfigStore` trait, injected at construction time (fixes test isolation).

**Files:** `crates/clawden-adapters/src/docker_adapter.rs` (new), all 5 adapter files reduced to metadata, `docker_runtime.rs` cleaned

### Phase 2: Provider & Channel Descriptor Registries (clawden-core)

Following the `RuntimeDescriptor` pattern from spec 041, create data-driven registries for providers and channels:

```rust
// crates/clawden-core/src/provider_registry.rs
pub struct ProviderDescriptor {
    pub name: &'static str,
    pub env_var: &'static str,
    pub test_endpoint: &'static str,
    pub display_name: &'static str,
}

pub static PROVIDERS: &[ProviderDescriptor] = &[
    ProviderDescriptor { name: "openai", env_var: "OPENAI_API_KEY", ... },
    ProviderDescriptor { name: "anthropic", env_var: "ANTHROPIC_API_KEY", ... },
    // ...
];

// crates/clawden-core/src/channel_registry.rs
pub struct ChannelDescriptor {
    pub channel_type: ChannelType,
    pub token_env_var: &'static str,
    pub required_credentials: &'static [&'static str],
    pub optional_credentials: &'static [&'static str],
}

pub static CHANNELS: &[ChannelDescriptor] = &[ /* ... */ ];
```

Replace all 8+ locations where `PROVIDER_ENV_CANDIDATES`, `CHANNEL_ENV_VARS`, `KNOWN_CHANNEL_TYPES`, and provider endpoint lists are hardcoded.

**Files:** `crates/clawden-core/src/provider_registry.rs` (new), `crates/clawden-core/src/channel_registry.rs` (new), all CLI command files that hardcode these lists

### Phase 3: `ManagerError` thiserror Enum (clawden-core, clawden-server)

Replace `Result<T, String>` in lifecycle manager with a proper error type:

```rust
#[derive(Debug, thiserror::Error)]
pub enum ManagerError {
    #[error("agent `{0}` not found")]
    AgentNotFound(String),
    #[error("invalid state transition from {from:?} to {to:?}")]
    InvalidTransition { from: AgentState, to: AgentState },
    #[error("adapter error for {runtime:?}: {source}")]
    Adapter { runtime: ClawRuntime, source: anyhow::Error },
    #[error("no adapter registered for {0:?}")]
    NoAdapter(ClawRuntime),
}
```

Extract the repeated "find agent or error" pattern (used 6 times) into a helper method.

**Files:** `crates/clawden-core/src/manager.rs`, `crates/clawden-server/src/manager.rs`, `crates/clawden-server/src/api.rs`

### Phase 4: Decompose God Functions (clawden-cli)

Break monolithic command functions into phase functions:

```rust
// exec_run decomposition:
fn load_and_merge_config(args: &RunArgs) -> Result<ResolvedConfig>;
fn resolve_execution_mode(config: &ResolvedConfig) -> ExecutionMode;
fn validate_credentials(config: &ResolvedConfig) -> Result<(), ValidationReport>;
fn launch_runtime(mode: ExecutionMode, config: ResolvedConfig) -> Result<RuntimeHandle>;
fn stream_and_wait(handle: RuntimeHandle, ctrlc: CancellationToken) -> Result<ExitStatus>;
```

Similarly for `exec_up` and `validate_direct_runtime_config`. Also consolidate duplicated helpers:
- `render_log_line()` — duplicated in up.rs, run.rs, logs.rs → shared util
- `infer_provider_from_host_env()` — duplicated in run.rs, up.rs → shared util
- `channel_token_env_name()` — 4+ locations → use channel registry from Phase 2
- `provider_key_env_names()` — 4+ locations → use provider registry from Phase 2

**Files:** `crates/clawden-cli/src/commands/run.rs`, `up.rs`, `config_gen.rs`, `init.rs`, `providers.rs`

### Phase 5: Format-Agnostic Config Emitter (clawden-cli)

Unify `inject_proxy_config()` / `inject_proxy_config_json()` and the 4 near-identical `RuntimeConfigTranslator` impls in clawden-config:

```rust
trait ConfigEmitter {
    fn set(&mut self, path: &[&str], value: ConfigValue);
    fn set_array(&mut self, path: &[&str], values: Vec<ConfigValue>);
}

struct TomlEmitter { doc: toml_edit::Document }
struct JsonEmitter { root: serde_json::Value }

// Written once, works for both formats
fn inject_proxy_config(emitter: &mut dyn ConfigEmitter, proxy: &ProxySettings) { ... }
```

**Files:** `crates/clawden-cli/src/commands/config_gen.rs`, `crates/clawden-config/src/lib.rs`

### Phase 6: Minor Cleanups

- Extract `current_unix_ms()` to `clawden_core::util` (duplicated in 4 files)
- Fix `process.rs` thread leak: add cancellation token to `stream_logs()` background thread
- Remove `#[allow(dead_code)]` items in `server/src/channels.rs` or implement them
- Unify credential redaction logic (3 implementations in api.rs and config.rs)
- Fix `dashboard` command to use `open` crate instead of macOS-only `Command::new("open")`

## Plan

- [ ] Phase 1: Generic `DockerAdapter<R>` and `ConfigStore` trait
- [x] Phase 2: Provider and channel descriptor registries in clawden-core
- [x] Phase 3: `ManagerError` thiserror enum for lifecycle manager
- [ ] Phase 4: Decompose exec_run, exec_up, validate_direct_runtime_config
- [ ] Phase 5: Format-agnostic config emitter
- [x] Phase 6: Minor cleanups (util extraction, thread leak, dead code, redaction, dashboard)

## Test

- [x] `cargo test -p clawden-core --quiet` passes
- [x] `cargo test -p clawden-cli --quiet` passes
- [x] `cargo test -p clawden-adapters --quiet` passes
- [x] `cargo test -p clawden-config --quiet` passes
- [x] `cargo test -p clawden-server --quiet` passes
- [x] `cargo clippy` clean across workspace
- [x] `cargo build --no-default-features --quiet` succeeds (feature-gated compilation)
- [ ] Adding a new adapter requires only a `RuntimeMeta` impl (~20 lines), not a 127-line file
- [ ] Adding a new provider requires 1 entry in `PROVIDERS` array, 0 match edits
- [ ] Adding a new channel requires 1 entry in `CHANNELS` array, 0 match edits
- [x] No behavioral regression in `clawden run`, `clawden up`, `clawden init` commands

## Notes

- Each phase is independently shippable. Phase 2 (registries) has the highest ROI and can land first.
- Phase 1 depends on the runtime-sync skill for adapter consistency validation.
- This spec explicitly excludes dashboard (React) refactoring — that's a separate concern.
- `SecretVault` XOR "encryption" is a known issue but out of scope — needs its own security-focused spec.
- The `ClawAdapter` trait monolith (11 methods) could be split into `Lifecycle + Health + Messaging + Config` sub-traits, but that's a larger API break better handled as a follow-up.
- Docker entrypoint.sh and Dockerfile match/case statements are shell scripts and out of scope.

- 2026-03-05: Implemented `ManagerError` in `clawden-core` lifecycle manager and switched server API error mapping to preserve typed manager errors while returning stable HTTP error strings.
- 2026-03-05: Implemented Phase 6 cleanups: extracted `current_unix_ms()` into `clawden-core::util`, wired core/server call sites, added `LogStream` cancellation to stop background log threads on drop, and made `dashboard` URL launching cross-platform via the `open` crate.
- 2026-03-05 verification: `cargo test -p clawden-core --quiet && cargo test -p clawden-cli --quiet && cargo test -p clawden-server --quiet && cargo test -p clawden-config --quiet && cargo test -p clawden-adapters --quiet`, `cargo clippy --workspace --quiet`, and `cargo build --workspace --no-default-features --quiet` all passed.