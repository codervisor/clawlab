---
name: runtime-research
description: >
  Research upstream claw runtimes using DeepWiki MCP tools to gather accurate metadata,
  track breaking changes, and align ClawDen's adapters and descriptors with upstream reality.
  Use when: (1) Adding a new runtime adapter or descriptor and need to gather upstream metadata
  (channels, config format, ports, capabilities, language, install method), (2) Checking whether
  an existing runtime's metadata is still accurate, (3) Investigating a specific upstream runtime's
  architecture, config options, or channel support for any ClawDen integration work, (4) Auditing
  adapter/descriptor alignment with upstream repos, (5) Answering questions about a claw runtime's
  features, breaking changes, or migration paths, (6) Working with any claw runtime integration —
  even if the user doesn't explicitly ask for "research", any runtime-related task benefits from
  checking upstream first. Requires: mcp_deepwiki MCP tools.
---

# Runtime Research

Research upstream claw runtimes using DeepWiki to keep ClawDen's integrations aligned
with the real upstream source of truth. This skill turns DeepWiki into a structured
research assistant for runtime metadata gathering, change detection, and alignment audits.

## Why Research First?

ClawDen maintains metadata for each runtime (channels, ports, config format, install
method, capabilities) across `RuntimeDescriptor` entries and `ClawAdapter` implementations.
This metadata drifts whenever an upstream runtime ships changes. Incorrect metadata causes:

- Failed installs (wrong archive format, missing prerequisites)
- Broken health checks (wrong port or endpoint)
- Missing channel support (new channels not exposed to users)
- Config generation errors (wrong format or field names)

Researching upstream **before** editing ClawDen code prevents these issues.

## Decision Tree

- **Adding a new runtime?** → Run the [full research workflow](#full-research-workflow) to
  gather all metadata, then apply findings to a `RuntimeDescriptor` entry. Only add a full
  adapter if the runtime needs lifecycle ops beyond what the descriptor provides.
- **Updating an existing runtime?** → Run a [targeted check](#targeted-research) to see
  what changed upstream since the last sync.
- **Investigating a runtime's capabilities?** → Use [exploratory research](#exploratory-research)
  to answer specific questions.
- **Auditing all runtimes?** → Run [alignment audit](#alignment-audit) across all known
  runtimes.

## DeepWiki Tool Reference

Three MCP tools are available for runtime research:

### `mcp_deepwiki_read_wiki_structure`

Returns the documentation outline for a repo. Use this first to understand what sections
exist and plan which to drill into.

```
mcp_deepwiki_read_wiki_structure(repoName: "owner/repo")
```

**When to use:** Starting research on any runtime — gives you the table of contents so
you know what documentation sections cover channels, config, deployment, etc.

### `mcp_deepwiki_read_wiki_contents`

Reads the full content of a specific wiki page. Use to get detailed documentation on a
specific topic (e.g., channel adapters, configuration reference, deployment).

```
mcp_deepwiki_read_wiki_contents(repoName: "owner/repo", pagePath: "4.2")
```

**When to use:** After `read_wiki_structure` identifies a relevant page — read it for
detailed information about channels, config options, CLI flags, etc.

### `mcp_deepwiki_ask_question`

Asks a natural-language question and gets an AI-powered answer grounded in the repo's code
and docs. This is the most flexible tool — use it for specific questions.

```
mcp_deepwiki_ask_question(
  repoName: "owner/repo",
  question: "What messaging channels does this runtime support?"
)
```

**When to use:** For targeted questions where you know what you need but don't want to
read through entire wiki pages. Also great for cross-cutting questions that span multiple
sections.

## Source Registry

These are the upstream repos for all known runtimes. Use the `repoName` values with
DeepWiki tools.

| Runtime | DeepWiki `repoName` | Type |
|---------|---------------------|------|
| **OpenClaw** | `openclaw/openclaw` | npm package (TypeScript) |
| **ZeroClaw** | `zeroclaw-labs/zeroclaw` | GitHub releases (Rust) |
| **PicoClaw** | `picoclaw-labs/picoclaw` | GitHub releases (Go) |
| **NanoClaw** | `qwibitai/nanoclaw` | Git clone (TypeScript) |
| **OpenFang** | `RightNow-AI/openfang` | GitHub releases (Rust) |
| **IronClaw** | TBD | Stub — not yet integrated |
| **NullClaw** | TBD | Stub — not yet integrated |
| **MicroClaw** | TBD | Stub — not yet integrated |
| **MimiClaw** | TBD | Stub — not yet integrated |

## Full Research Workflow

Use this when adding a new runtime or doing a comprehensive sync of an existing one.

### Step 1: Get the documentation map

```
mcp_deepwiki_read_wiki_structure(repoName: "{owner}/{repo}")
```

Scan the outline for sections covering:
- Architecture / overview → language, capabilities
- Configuration → config format, default port, env vars
- Channels / integrations → supported messaging platforms
- Deployment / CLI → install method, start args, health endpoint
- API → default port, endpoints

### Step 2: Ask targeted metadata questions

Ask focused questions to extract the specific fields ClawDen needs:

```
mcp_deepwiki_ask_question(
  repoName: "{owner}/{repo}",
  question: "What messaging channels/platforms does this support? For each channel, is it native built-in support or via an external bridge/library?"
)
```

```
mcp_deepwiki_ask_question(
  repoName: "{owner}/{repo}",
  question: "What is the config file format (TOML, JSON, YAML, env vars)? What is the default config file name and location? What is the default API/health check port?"
)
```

```
mcp_deepwiki_ask_question(
  repoName: "{owner}/{repo}",
  question: "What is the install method? Is there a binary release on GitHub, an npm package, a cargo crate, or does it need to be built from source? What are the CLI start arguments?"
)
```

### Step 3: Read detailed pages for specifics

If any answers need more detail, read the relevant wiki pages identified in Step 1:

```
mcp_deepwiki_read_wiki_contents(repoName: "{owner}/{repo}", pagePath: "{section}")
```

Common pages to read:
- Configuration reference → exact config fields, env var names
- Channel adapters → per-channel setup requirements
- CLI reference → all subcommands and flags
- Deployment → Docker, systemd, or other deployment methods

### Step 4: Map findings to ClawDen fields

Translate research results into the fields ClawDen needs:

| Research finding | Maps to |
|-----------------|---------|
| Implementation language | `RuntimeDescriptor.language` / adapter `metadata().language` |
| Supported channels | `channel_support` HashMap in adapter `metadata()` |
| Config file format | `RuntimeDescriptor.config_format` |
| Default port | `RuntimeDescriptor.health_port` / adapter `metadata().default_port` |
| Install method | `RuntimeDescriptor.install_source` |
| Version query method | `RuntimeDescriptor.version_source` |
| CLI start args | `RuntimeDescriptor.default_start_args` |
| Config dir support | `RuntimeDescriptor.supports_config_dir` + `config_dir_flag` |
| Capabilities | adapter `metadata().capabilities` |

### Step 5: Document discrepancies

Compare findings against the current `RuntimeDescriptor` and adapter metadata. Note any
differences — these are the changes needed to align ClawDen with upstream.

## Targeted Research

Quick check on a specific aspect of a runtime. Use when you have a focused question.

```
mcp_deepwiki_ask_question(
  repoName: "{owner}/{repo}",
  question: "{your specific question}"
)
```

**Example questions:**

- "What changed in the latest release? Any new channels, config changes, or breaking changes?"
- "Does this runtime support Feishu/Lark? If so, how is it configured?"
- "What environment variables does this runtime read for configuration?"
- "What is the health check endpoint and expected response format?"
- "Does this runtime have a built-in onboarding/setup command?"

## Exploratory Research

When investigating a runtime's architecture or capabilities without a specific goal:

1. Start with `read_wiki_structure` to see what's documented
2. Read the Overview page for a high-level understanding
3. Ask follow-up questions based on what you learn
4. Read specific pages that are relevant to ClawDen integration

This is useful for evaluating whether a new runtime should be added to ClawDen, or for
understanding how a runtime works internally to debug integration issues.

## Alignment Audit

To audit whether ClawDen's metadata matches upstream reality across all runtimes:

1. For each runtime with a known upstream repo, run targeted checks:
   ```
   mcp_deepwiki_ask_question(
     repoName: "{owner}/{repo}",
     question: "What are the supported channels, config format, default port, and implementation language?"
   )
   ```

2. Compare each response against the corresponding `RuntimeDescriptor` in
   `crates/clawden-core/src/runtime_descriptor.rs` and adapter `metadata()` in
   `crates/clawden-adapters/src/{slug}.rs`

3. Flag any mismatches as alignment issues to fix

## Where Findings Get Applied

After research, the findings map to specific ClawDen files:

| Layer | File | What to update |
|-------|------|---------------|
| Descriptor | `crates/clawden-core/src/runtime_descriptor.rs` | Metadata fields in `DESCRIPTORS` array |
| Adapter metadata | `crates/clawden-adapters/src/{slug}.rs` | `metadata()` method return values |
| Core enum | `crates/clawden-core/src/lib.rs` | `ClawRuntime` variant (new runtimes only) |
| Features | `crates/clawden-adapters/Cargo.toml` | Feature flag (new runtimes only) |
| Registry | `crates/clawden-adapters/src/lib.rs` | Module + registry entry (new runtimes only) |
| Docker | `docker/Dockerfile` | Version ARG + install (if Docker-supported) |
| Dashboard | `dashboard/src/components/runtimes/RuntimeCatalog.tsx` | Display metadata |

**Descriptor-driven files (auto-consume descriptor changes, no per-runtime edits):**

| File | Fields consumed |
|------|----------------|
| `crates/clawden-core/src/install.rs` | `install_source`, `version_source`, `default_start_args` |
| `crates/clawden-core/src/process.rs` | `health_port` |
| `crates/clawden-cli/src/commands/config_gen.rs` | `config_format`, `config_dir_flag` |
| `crates/clawden-core/src/manager.rs` | `cost_tier` |

## References

For implementation guidance after completing research:

- **[adapter-template.md](references/adapter-template.md)** — Canonical Rust adapter with
  every method annotated. Copy-paste source for new adapters.
- **[full-stack-checklist.md](references/full-stack-checklist.md)** — Step-by-step checklist
  from core enum through Docker and dashboard.
- **[consistency-rules.md](references/consistency-rules.md)** — Hard rules for adapter
  consistency, known violations, and audit procedure.
