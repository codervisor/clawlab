# Consistency Rules

Rules that MUST be enforced when modifying any runtime adapter.

## Table of Contents

- [Hard Rules](#hard-rules)
- [Known Violations to Fix](#known-violations-to-fix)
- [Audit Procedure](#audit-procedure)

## Hard Rules

### R1: send() uses echo pattern

Every adapter's `send()` returns `Ok(AgentResponse { content: format!("{Name} echo: {}", message.content) })`.
Never use `bail!()` or return an error — the method must succeed with an echo until real
runtime communication is implemented.

### R2: get_config() fallback includes runtime key

When no stored config exists, return `serde_json::json!({ "runtime": "{slug}" })`.
Never return empty `{}` — the orchestrator relies on the runtime key for routing.

### R3: Every adapter has a config persistence test

The `start_persists_forwarded_runtime_config` test verifies that `start()` stores config
and `get_config()` retrieves it. Copy the test from `references/adapter-template.md`.

### R4: Identical method bodies for shared behavior

These methods MUST be identical across all adapters (only the runtime variant/slug differs):
- `install()` → `Ok(())`
- `start()` → `start_container` + `set_stored_config`
- `stop()` → `stop_container` + `remove_stored_config`
- `restart()` → `restart_container`
- `health()` → `container_running` check
- `metrics()` → zero stub
- `subscribe()` → `Ok(vec![])`
- `set_config()` → `set_stored_config`
- `list_skills()` → `Ok(vec![])`
- `install_skill()` → `Ok(())`

### R5: No bail! import in adapter modules

Only `anyhow::Result` should be imported. If `bail` appears, the adapter has an
inconsistent error path.

### R6: config_store() pattern is identical

Every adapter declares the same `config_store()` function with `OnceLock<Mutex<HashMap>>`.
Copy exactly — do not use alternative patterns.

### R7: Feature flag naming matches slug

The Cargo feature name, module filename, and `as_slug()` return value must all match.
Example: feature `zeroclaw`, file `zeroclaw.rs`, slug `"zeroclaw"`.

## Known Violations to Fix

When working on any adapter, fix these if encountered:

| Adapter | Violation | Rule | Fix |
|---------|-----------|------|-----|
| OpenClaw | `send()` uses `bail!("OpenClawAdapter.send not implemented")` | R1 | Replace with echo pattern |
| OpenClaw | `get_config()` fallback returns `serde_json::json!({})` | R2 | Add `"runtime": "openclaw"` |
| OpenClaw | Imports `bail` from anyhow | R5 | Remove `bail` import |
| OpenClaw | No test module | R3 | Add config persistence test |
| PicoClaw | No test module | R3 | Add config persistence test |
| NanoClaw | No test module | R3 | Add config persistence test |

## Audit Procedure

To audit all adapters for consistency:

1. Open all adapter files side-by-side:
   ```
   crates/clawden-adapters/src/zeroclaw.rs
   crates/clawden-adapters/src/openclaw.rs
   crates/clawden-adapters/src/picoclaw.rs
   crates/clawden-adapters/src/nanoclaw.rs
   ```

2. For each method in the ClawAdapter trait, verify:
   - Method body matches the canonical template (ignoring variant/slug substitution)
   - No extra imports beyond the standard set
   - No bail!() calls anywhere

3. Verify each adapter has `#[cfg(test)] mod tests` with config persistence test

4. Verify `crates/clawden-adapters/src/lib.rs` has matching:
   - `#[cfg(feature = "{slug}")] mod {slug};`
   - `#[cfg(feature = "{slug}")] pub use {slug}::{Name}Adapter;`
   - `#[cfg(feature = "{slug}")] registry.register(...)` inside `builtin_registry()`

5. Verify `crates/clawden-adapters/Cargo.toml` has `{slug} = []` and `"{slug}"` in `default`
