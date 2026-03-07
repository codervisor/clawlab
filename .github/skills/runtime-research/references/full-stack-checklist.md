# Full-Stack Checklist: Adding a New Runtime

Follow every step in order. Do not skip steps.

## Table of Contents

- [Step 0: Upstream research](#step-0-upstream-research)
- [Step 1: Core enum](#step-1-core-enum)
- [Step 2: Runtime descriptor](#step-2-runtime-descriptor)
- [Step 3: Adapter module](#step-3-adapter-module)
- [Step 4: Feature flag](#step-4-feature-flag)
- [Step 5: Registry wiring](#step-5-registry-wiring)
- [Step 6: Docker integration](#step-6-docker-integration)
- [Step 7: Dashboard](#step-7-dashboard)
- [Step 8: Verify](#step-8-verify)

## Step 0: Upstream research

**Reference:** [upstream-sources.md](upstream-sources.md)

Before writing any code, research the runtime's upstream repo/registry:

1. Find the runtime's source (GitHub repo, npm package, etc.)
2. Read the README for: channels, config format, default port, capabilities, language
3. Check recent CHANGELOG/releases for new features or breaking changes
4. Determine the install method (binary release, npm, git clone, cargo)
5. Document channel support levels (Native, Via("mechanism"), Unsupported)

## Step 1: Core enum

**File:** `crates/clawden-core/src/lib.rs`

Add variant to `ClawRuntime` enum:

1. Add variant to `enum ClawRuntime` (alphabetical within the existing group)

> **Note:** `Display`, `from_str_loose`, and `as_slug` are now derived from the
> `RuntimeDescriptor` entry (slug, display_name, aliases). No manual match arms needed.

## Step 2: Runtime descriptor

**File:** `crates/clawden-core/src/runtime_descriptor.rs`

Add an entry to the `DESCRIPTORS` static array. This is the **single source of truth**
for all per-runtime metadata consumed by `install.rs`, `process.rs`, `config_gen.rs`,
and `manager.rs`.

```rust
RuntimeDescriptor {
    runtime: ClawRuntime::{Variant},
    slug: "{slug}",
    display_name: "{Name}",
    aliases: &["{slug-with-dash}", "{short}"],
    install_source: InstallSource::GithubRelease {
        owner: "{owner}",
        repo: "{repo}",
        archive_ext: ".tar.gz",
    },
    version_source: VersionSource::GithubLatest {
        owner: "{owner}",
        repo: "{repo}",
    },
    direct_install_supported: true,
    default_start_args: &["daemon"],
    subcommand_hints: &[],
    config_format: ConfigFormat::Toml,  // or Json, EnvVars, None
    supports_config_dir: true,
    config_dir_flag: ConfigDirFlag::ConfigDir,
    has_onboard_command: false,
    health_port: Some({port}),  // or None
    cost_tier: 2,
},
```

Fields reference:

| Field | Purpose | Used by |
|-------|---------|---------|
| `install_source` | How to install the binary | `install.rs` |
| `version_source` | How to query latest version | `install.rs` |
| `direct_install_supported` | Whether `clawden install` works | `install.rs` |
| `default_start_args` | Args passed to runtime binary on start | `install.rs`, `entrypoint.sh` |
| `subcommand_hints` | Tab-completion / help hints | CLI |
| `config_format` | Toml/Json/EnvVars/None | `config_gen.rs` |
| `supports_config_dir` | Whether runtime accepts a config dir | `config_gen.rs` |
| `config_dir_flag` | `--config-dir` vs `--config <file>` | `config_gen.rs` |
| `has_onboard_command` | Whether runtime has an onboard subcommand | `config_gen.rs` |
| `health_port` | Port for health check URL | `process.rs` |
| `cost_tier` | Cost ranking (1=cheap, 3=expensive) | `manager.rs` |

> For stub runtimes without upstream sources, use `InstallSource::NotAvailable`,
> `VersionSource::NotAvailable`, and `direct_install_supported: false`.

## Step 3: Adapter module

**File:** Create `crates/clawden-adapters/src/{slug}.rs`

> **Note:** This step is optional for stub runtimes. If the runtime only needs
> metadata (install, config, health) without lifecycle adapter ops, the descriptor
> from Step 2 is sufficient. Skip Steps 3–5 for descriptor-only runtimes.

Follow the canonical template in `references/adapter-template.md` exactly.

Checklist:
- [ ] Struct `{Name}Adapter` with no fields
- [ ] `config_store()` function (copy exactly from template)
- [ ] `metadata()` with correct runtime, language, capabilities, port, config_format, channels
- [ ] All lifecycle methods use `ClawRuntime::{Variant}` and `"{slug}"`
- [ ] `send()` uses echo pattern (NOT `bail!`)
- [ ] `get_config()` fallback includes `"runtime": "{slug}"` (NOT empty `{}`)
- [ ] Tests module with `start_persists_forwarded_runtime_config` test

## Step 4: Feature flag

**File:** `crates/clawden-adapters/Cargo.toml`

> Skip this step if no adapter was created in Step 3.

1. Add `{slug} = []` to `[features]` section
2. Add `"{slug}"` to `default` feature list

## Step 5: Registry wiring

**File:** `crates/clawden-adapters/src/lib.rs`

> Skip this step if no adapter was created in Step 3.

Add three blocks:

```rust
// At top with other mod declarations:
#[cfg(feature = "{slug}")]
mod {slug};

// With other pub use statements:
#[cfg(feature = "{slug}")]
pub use {slug}::{Name}Adapter;

// Inside builtin_registry() function:
#[cfg(feature = "{slug}")]
registry.register(ClawRuntime::{Variant}, Arc::new({Name}Adapter));
```

## Step 6: Docker integration

### Dockerfile (`docker/Dockerfile`)

1. Add version ARG: `ARG {SLUG_UPPER}_VERSION=latest`
2. Add install command: `&& clawden-cli install "{slug}@${{{SLUG_UPPER}_VERSION}}"`

### Entrypoint (`docker/entrypoint.sh`)

Add to the runtime case statement:

```bash
{slug})
    DEFAULT_ARGS="{default_args}"  # "daemon", "gateway", or ""
    ;;
```

## Step 7: Dashboard

**File:** `dashboard/src/components/runtimes/RuntimeCatalog.tsx`

If the runtime uses a language not already in `LANGUAGE_COLORS`:
1. Add color entry for the new language

No other dashboard changes needed — the catalog auto-discovers runtimes from the `/api/runtimes` API.

## Step 8: Verify

Run these checks in order:

```bash
# 1. Compile with the new feature (skip if no adapter)
cargo build --features {slug}

# 2. Run core tests (descriptor coverage)
cargo test -p clawden-core --quiet

# 3. Run adapter tests (skip if no adapter)
CLAWDEN_ADAPTER_DRY_RUN=1 cargo test -p clawden-adapters

# 4. Run CLI tests
cargo test -p clawden-cli --quiet

# 5. Clippy
cargo clippy --workspace --all-features -- -D warnings

# 6. Format check
cargo fmt --check
```
