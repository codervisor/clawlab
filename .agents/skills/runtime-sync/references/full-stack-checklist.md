# Full-Stack Checklist: Adding a New Runtime

Follow every step in order. Do not skip steps.

## Table of Contents

- [Step 0: Upstream research](#step-0-upstream-research)
- [Step 1: Core enum](#step-1-core-enum)
- [Step 2: Adapter module](#step-2-adapter-module)
- [Step 3: Feature flag](#step-3-feature-flag)
- [Step 4: Registry wiring](#step-4-registry-wiring)
- [Step 5: Docker integration](#step-5-docker-integration)
- [Step 6: Dashboard](#step-6-dashboard)
- [Step 7: Verify](#step-7-verify)

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

Add variant to `ClawRuntime` enum and update all match arms:

1. Add variant to `enum ClawRuntime` (alphabetical within the existing group)
2. Add `Display` impl arm: `ClawRuntime::{Variant} => write!(f, "{Name}")`
3. Add `from_str_loose` arm: `"{slug}" | "{slug-with-dash}" | "{short}" => Some(Self::{Variant})`
4. Add `as_slug` arm: `ClawRuntime::{Variant} => "{slug}"`

## Step 2: Adapter module

**File:** Create `crates/clawden-adapters/src/{slug}.rs`

Follow the canonical template in `references/adapter-template.md` exactly.

Checklist:
- [ ] Struct `{Name}Adapter` with no fields
- [ ] `config_store()` function (copy exactly from template)
- [ ] `metadata()` with correct runtime, language, capabilities, port, config_format, channels
- [ ] All lifecycle methods use `ClawRuntime::{Variant}` and `"{slug}"`
- [ ] `send()` uses echo pattern (NOT `bail!`)
- [ ] `get_config()` fallback includes `"runtime": "{slug}"` (NOT empty `{}`)
- [ ] Tests module with `start_persists_forwarded_runtime_config` test

## Step 3: Feature flag

**File:** `crates/clawden-adapters/Cargo.toml`

1. Add `{slug} = []` to `[features]` section
2. Add `"{slug}"` to `default` feature list

## Step 4: Registry wiring

**File:** `crates/clawden-adapters/src/lib.rs`

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

## Step 5: Docker integration

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

## Step 6: Dashboard

**File:** `dashboard/src/components/runtimes/RuntimeCatalog.tsx`

If the runtime uses a language not already in `LANGUAGE_COLORS`:
1. Add color entry for the new language

No other dashboard changes needed — the catalog auto-discovers runtimes from the `/api/runtimes` API.

## Step 7: Verify

Run these checks in order:

```bash
# 1. Compile with the new feature
cargo build --features {slug}

# 2. Run adapter tests
CLAWDEN_ADAPTER_DRY_RUN=1 cargo test -p clawden-adapters

# 3. Run full test suite
cargo test --workspace

# 4. Clippy
cargo clippy --workspace --all-features -- -D warnings

# 5. Format check
cargo fmt --check
```
