---
name: runtime-sync
description: >
  Enforce consistency across ClawDen runtime adapter implementations and guide adding
  new runtimes to the claw ecosystem. Use when: (1) Adding a new runtime adapter
  (e.g., OpenFang, IronClaw, NullClaw, MimiClaw), (2) Modifying any existing adapter
  in crates/clawden-adapters/src/, (3) Auditing adapters for consistency issues,
  (4) Fixing inconsistencies across runtime integrations, (5) Working with any file
  matching *claw*.rs in the adapters crate, (6) Updating docker/Dockerfile or
  docker/entrypoint.sh for runtime support, (7) Touching dashboard RuntimeCatalog
  component for runtime display.
---

# Runtime Sync

Enforce consistency across ClawDen's `ClawAdapter` implementations. All runtime adapters
follow an identical structure — only metadata values (language, channels, ports) differ.

## Decision Tree

- **Adding a new runtime?** → First run the [upstream research workflow](references/upstream-sources.md#research-workflow)
  to gather accurate metadata, then follow [full-stack-checklist.md](references/full-stack-checklist.md)
  using [adapter-template.md](references/adapter-template.md) for the canonical Rust pattern
- **Updating an existing adapter?** → Check [upstream-sources.md](references/upstream-sources.md)
  for the runtime's repo/npm, review recent releases for changes, then update metadata
- **Modifying adapter behavior?** → Read [consistency-rules.md](references/consistency-rules.md)
  first, fix any known violations you encounter
- **Auditing all adapters?** → Follow the audit procedure in [consistency-rules.md](references/consistency-rules.md)

## Upstream Research (Do This First)

Before writing any adapter code, research the runtime's upstream repo/registry to get
accurate metadata. The [upstream-sources.md](references/upstream-sources.md) reference documents:

- **Source registry** — GitHub repo URLs, npm packages, and git repos for all 9 runtimes
- **5-step research workflow** — README → CHANGELOG → channel details → install method → document
- **Quick commands** — curl/npm/git one-liners to check latest versions and READMEs
- **Install logic cross-reference** — how each runtime maps to `install.rs` functions

Key repos:
| Runtime | Upstream |
|---------|----------|
| ZeroClaw | `zeroclaw-labs/zeroclaw` (GitHub releases) |
| PicoClaw | `picoclaw-labs/picoclaw` (GitHub releases) |
| OpenClaw | `openclaw` npm package |
| NanoClaw | `qwibitai/nanoclaw` (git clone) |

## Architecture Quick Reference

All adapters live in `crates/clawden-adapters/src/` and implement the `ClawAdapter` trait
from `crates/clawden-core/src/lib.rs`. They delegate container execution to shared helpers
in `docker_runtime.rs`.

**Currently implemented:** ZeroClaw (Rust), OpenClaw (TypeScript), PicoClaw (Go), NanoClaw (TypeScript)

**Defined but unimplemented:** IronClaw, NullClaw, MicroClaw, MimiClaw

**Files that reference runtimes:**

| Layer | File | What to update |
|-------|------|---------------|
| Core enum | `crates/clawden-core/src/lib.rs` | `ClawRuntime` enum + Display/from_str_loose/as_slug |
| Adapter | `crates/clawden-adapters/src/{slug}.rs` | New module implementing `ClawAdapter` |
| Features | `crates/clawden-adapters/Cargo.toml` | Feature flag + default list |
| Registry | `crates/clawden-adapters/src/lib.rs` | mod, pub use, builtin_registry() |
| Docker | `docker/Dockerfile` | Version ARG + install command |
| Entrypoint | `docker/entrypoint.sh` | Runtime case statement |
| Dashboard | `dashboard/src/components/runtimes/RuntimeCatalog.tsx` | Language colors (only if new language) |

## Critical Consistency Rules

1. **`send()` uses echo pattern** — `Ok(AgentResponse { content: format!("{Name} echo: ...") })`, never `bail!()`
2. **`get_config()` fallback includes runtime key** — `json!({ "runtime": "{slug}" })`, never empty `{}`
3. **Every adapter has tests** — `start_persists_forwarded_runtime_config` test
4. **No `bail!` import** — only `anyhow::Result`
5. **Identical method bodies** — lifecycle, health, metrics, subscribe, skills are copy-paste with variant substitution

See [consistency-rules.md](references/consistency-rules.md) for the complete rule set and known violations.

## References

- **[upstream-sources.md](references/upstream-sources.md)** — Upstream repo URLs for all runtimes, 5-step research workflow, quick CLI commands, and install logic cross-reference. **Read this first** before any adapter work.
- **[adapter-template.md](references/adapter-template.md)** — Canonical Rust adapter with every method annotated. Use as copy-paste source for new adapters.
- **[full-stack-checklist.md](references/full-stack-checklist.md)** — Step-by-step checklist covering core enum → adapter → Cargo features → registry → Docker → dashboard → verification.
- **[consistency-rules.md](references/consistency-rules.md)** — Hard rules, known violations in existing adapters, and audit procedure.
