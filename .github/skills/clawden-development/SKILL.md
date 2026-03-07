---
name: clawden-development
description: Guidance for implementing and reviewing changes in ClawDen. Use this skill whenever modifying ClawDen Rust, TypeScript, dashboard, SDK, CLI, config, tests, or docs, especially for refactors, bug fixes, command behavior changes, or cross-crate work. Prefer this skill by default for repository development tasks unless a more specific ClawDen skill applies.
---

# ClawDen Development

Use this skill for day-to-day engineering work in this repository. Keep changes small, rooted in the actual architecture, and validated with focused checks.

## Start Here

1. Read `AGENTS.md` before making assumptions.
2. If the task touches specs or is a multi-step design change, load `leanspec-sdd` first.
3. If the task touches runtime adapters or runtime catalog consistency, load `runtime-sync` first.
4. Inspect the relevant code paths before editing.

## Core Rules

- Fix the underlying cause instead of layering command-specific workarounds.
- Prefer one shared path over repeated per-command logic.
- Preserve existing public behavior unless the task explicitly changes it.
- Keep diffs narrow; avoid cleanup unrelated to the task.
- Never store secrets in plain text or add logging that leaks credentials.
- Maintain audit logging for lifecycle-affecting behavior.

## Repository Expectations

- Backend changes are Rust-first and should satisfy `rustfmt` and clippy expectations.
- Dashboard and SDK changes should preserve the established React and TypeScript patterns already in the repo.
- Runtime metadata belongs in descriptors when possible; avoid reintroducing scattered per-runtime match logic.
- Adapter work should stay consistent across runtimes rather than growing bespoke implementations.

## CLI Change Checklist

When changing `clawden` command behavior:

1. Check whether the behavior already exists in another command and unify it.
2. Make environment, config, and flag precedence explicit.
3. Handle both `clawden.yaml` and env-only workflows where the command supports them.
4. Add a regression test for the user-visible behavior.

## Validation

Choose the smallest test set that proves the change:

- Single command behavior: run the relevant `cargo test -p clawden-cli --bin clawden` tests.
- Cross-crate Rust change: run targeted crate tests first, then expand only if necessary.
- Refactors affecting shared code paths: add at least one regression test that exercises the shared entrypoint.

If a test is intentionally long-running or flaky, mark it clearly and keep the default test path fast.

## Review Standard

Before finishing, verify:

1. The implementation is centralized rather than duplicated.
2. The changed path has a direct test or a justified manual verification step.
3. Error messages remain actionable.
4. No secrets, tokens, or credentials are exposed in stdout, stderr, or logs.