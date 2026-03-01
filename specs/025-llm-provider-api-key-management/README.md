---
status: planned
created: 2026-03-01
priority: medium
tags:
- config
- secrets
- llm
- api-keys
- security
depends_on:
- 013-config-management
parent: 009-orchestration-platform
created_at: 2026-03-01T08:47:48.955288Z
updated_at: 2026-03-01T08:48:01.471949Z
---
# LLM Provider API Key Management

## Overview

ClawDen's `clawden.yaml` schema covers runtimes and channels well, but has no typed support for LLM provider API keys. Users currently can't configure their OpenAI, Anthropic, Google, or other LLM credentials in the YAML — model config is only accessible via the untyped `config` bag, and `api_key_ref` only resolves through `SecretVault`, not via `$ENV_VAR` syntax like channel tokens do.

This spec adds a first-class `providers` section to `clawden.yaml` for configuring LLM provider credentials, plus CLI commands and config pipeline updates.

## Context

### Why "Providers" Not "Models"

API keys, base URLs, and org IDs are all scoped to the **provider** (OpenAI, Anthropic, Google), not the model (gpt-4o, claude-sonnet). One OpenAI key works for all OpenAI models. The model choice is separate — it's a runtime-level setting that selects which model to use within a provider.

This matches how the real SDKs work:
- **Anthropic SDK**: `new Anthropic()` reads `ANTHROPIC_API_KEY` — provider-level. Model is per-request.
- **OpenAI SDK**: `new OpenAI({ apiKey })` — provider-level. Model is per-request.
- **Open Interpreter (OpenClaw)**: `interpreter.llm.api_key` + `interpreter.llm.model` — provider config separate from model selection.

### What Works Today

- **Channel tokens**: `$ENV_VAR` syntax in YAML, auto-resolved from environment / `.env` file
- **`ModelConfig` struct**: exists with `provider`, `name`, `api_key_ref` — but only reachable via the untyped `config` HashMap in `ClawDenYaml`
- **`SecretVault`**: resolves `api_key_ref` references, but uses XOR placeholder crypto

### What's Missing

1. No `providers` section in `clawden.yaml` — users can't declare LLM providers in a typed way
2. `$ENV_VAR` resolution doesn't apply to `api_key_ref` — only channel tokens get env-var expansion
3. `provider` is a free-form `String` — no validation, no known-provider defaults
4. No provider-specific fields (base URL, API version, org ID) for self-hosted or proxy setups
5. `OpenClawConfigTranslator` drops `api_key_ref` — inconsistent across translators
6. No provider validation command — channels have `clawden channels test`, but there's no equivalent for LLM providers

## Design

### User-Facing Config: `clawden.yaml`

Add a top-level `providers` section. Runtimes reference a provider by name.

```yaml
# clawden.yaml — multi-runtime with shared providers

providers:
  openai:
    api_key: $OPENAI_API_KEY
  anthropic:
    api_key: $ANTHROPIC_API_KEY
  local-llm:
    type: openai                         # OpenAI-compatible API
    api_key: $LOCAL_LLM_KEY
    base_url: "http://localhost:11434/v1"

channels:
  telegram:
    type: telegram
    token: $TELEGRAM_BOT_TOKEN

runtimes:
  - name: zeroclaw
    provider: openai                     # references providers.openai
    model: gpt-4o                        # model selection within provider
    channels: [telegram]
    tools: [git]

  - name: nanoclaw
    provider: anthropic
    model: claude-sonnet-4-20250514
    channels: [telegram]
```

Single-runtime shorthand:

```yaml
runtime: zeroclaw
provider:
  type: openai
  api_key: $OPENAI_API_KEY
model: gpt-4o
channels:
  telegram:
    token: $TELEGRAM_BOT_TOKEN
```

Minimal — well-known provider with convention-based key:

```yaml
runtime: zeroclaw
provider: openai                        # infers $OPENAI_API_KEY from env
model: gpt-4o
channels:
  telegram:
    token: $TELEGRAM_BOT_TOKEN
```

### Provider Entry Schema

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderEntryYaml {
    #[serde(rename = "type")]
    pub provider_type: Option<LlmProvider>,  // inferred from key name if absent
    pub api_key: Option<String>,             // $ENV_VAR or vault ref
    pub base_url: Option<String>,            // override for proxies / self-hosted
    pub org_id: Option<String>,              // provider org/project ID
    pub extra: HashMap<String, Value>,       // provider-specific extensions
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LlmProvider {
    OpenAi,
    Anthropic,
    Google,
    Mistral,
    Groq,
    OpenRouter,
    Ollama,
    Custom(String),
}
```

### Provider Defaults

Known providers carry sensible defaults so users only specify what differs:

| Provider   | Default Base URL                            | Default Key Env Var    |
| ---------- | ------------------------------------------- | ---------------------- |
| OpenAI     | `https://api.openai.com/v1`                 | `OPENAI_API_KEY`       |
| Anthropic  | `https://api.anthropic.com`                 | `ANTHROPIC_API_KEY`    |
| Google     | `https://generativelanguage.googleapis.com` | `GOOGLE_API_KEY`       |
| Mistral    | `https://api.mistral.ai/v1`                 | `MISTRAL_API_KEY`      |
| Groq       | `https://api.groq.com/openai/v1`            | `GROQ_API_KEY`         |
| OpenRouter | `https://openrouter.ai/api/v1`              | `OPENROUTER_API_KEY`   |
| Ollama     | `http://localhost:11434/v1`                 | — (typically local)    |

When a user writes `provider: openai` with no `api_key`, ClawDen looks up `$OPENAI_API_KEY` from the environment automatically.

### Model Selection

Model is a **separate field** at the runtime level, not part of provider config:

```yaml
runtimes:
  - name: zeroclaw
    provider: openai          # credentials + endpoint
    model: gpt-4o             # which model to use
```

If `model` is omitted, runtimes use their own default. ClawDen does not enforce a default model per provider — that's the runtime's responsibility.

### `$ENV_VAR` Resolution Unification

Extend `resolve_env_vars()` to cover `ProviderEntryYaml.api_key` and `ProviderEntryYaml.base_url`, not just channel tokens. One code path for all env-var expansion.

### Runtime Translator Updates

All CRI config translators must consistently map provider credentials + model:

- **OpenClawConfigTranslator**: include `api_key` in runtime JSON (currently drops it); map to LiteLLM format (`provider/model`)
- **ZeroClawConfigTranslator**: map to `ZEROCLAW_LLM_API_KEY` env var + TOML `[model]` section
- **PicoClawConfigTranslator**: map to `llm.apiKey` + `llm.model` in JSON config
- **NanoClawConfigTranslator**: pass as `ANTHROPIC_API_KEY` / `OPENAI_API_KEY` env var per SDK convention

### CLI Commands

```bash
# List configured providers + connection status
clawden providers

# Test API key validity (lightweight API call per provider)
clawden providers test
clawden providers test openai

# Set a key interactively (stores in SecretVault)
clawden providers set-key openai
```

### Security

- API keys follow existing secret conventions: `$ENV_VAR` in YAML, `.env` auto-load, `SecretVault` for persistent storage
- Keys redacted in all logs, `clawden ps`, dashboard displays, API responses (existing `to_safe_json()` pattern)
- `clawden providers` shows provider name + status but never the key
- Keys never written to `clawden.yaml` by any CLI command

### Dashboard Integration

The Runtime Instance Manager (spec 021) should show provider + model per runtime. Config editor supports the `providers` section with `api_key` fields masked.

## Plan

- [ ] Add `LlmProvider` enum and `ProviderEntryYaml` struct to `clawden-config`
- [ ] Add `providers` section to `ClawDenYaml` schema with parsing + validation
- [ ] Add `provider` and `model` fields to `RuntimeEntryYaml`
- [ ] Implement provider defaults lookup (base URL, env var convention)
- [ ] Extend `resolve_env_vars()` to cover provider `api_key` and `base_url` fields
- [ ] Support `provider: <name>` string shorthand in YAML (single-runtime sugar)
- [ ] Support `provider: <name>` reference to `providers.<name>` entry (multi-runtime)
- [ ] Update `OpenClawConfigTranslator` to include API key in runtime output
- [ ] Ensure all four Phase 1 translators consistently map provider credentials + model
- [ ] Add `clawden providers` CLI command (list providers + status)
- [ ] Add `clawden providers test` CLI command (validate keys via lightweight API call)
- [ ] Add `clawden providers set-key <provider>` CLI command (interactive, stores in vault)
- [ ] Update dashboard config editor to support `providers` section with masked keys
- [ ] Add unit tests: YAML parsing, env-var resolution, provider defaults, translator output
- [ ] Add integration test: end-to-end provider config → runtime receives correct key

## Test

- [ ] `provider: openai` shorthand resolves `$OPENAI_API_KEY` from environment without explicit `api_key` field
- [ ] `providers` section with multiple entries parses correctly; each runtime references the right one
- [ ] `$ENV_VAR` in `api_key` resolves identically to channel token resolution
- [ ] Provider defaults populate `base_url` when not specified
- [ ] `Custom` provider type requires explicit `base_url`
- [ ] `clawden providers test` returns success/failure per provider
- [ ] API keys never appear in logs, `clawden ps` output, or API responses
- [ ] All four Phase 1 translators include provider credentials in their output
- [ ] Invalid provider type is rejected with a clear error message
- [ ] `.env` file keys are picked up for providers
- [ ] `model` field is passed to runtime separately from provider credentials

## Notes

- `Custom(String)` type supports OpenAI-compatible endpoints (LM Studio, vLLM, Ollama, etc.)
- `Ollama` has no default API key — typically local and unauthenticated
- The `providers` section is optional — single-runtime shorthand (`provider: openai`) doesn't require it
- Key rotation and TTL are out of scope — tracked separately if needed
- Real encryption for `SecretVault` (replacing XOR placeholder) is a prerequisite but covered by spec 013
- Provider config is independent of model selection — one provider can serve many models, and the same model name may exist across providers