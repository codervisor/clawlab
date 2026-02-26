---
status: complete
created: 2026-02-26
priority: high
tags:
- npm
- publishing
- cli
- sdk
- distribution
depends_on:
- 015-developer-sdk
- 016-project-setup
parent: 009-orchestration-platform
created_at: 2026-02-26T06:05:43.448655328Z
updated_at: 2026-02-26T07:24:08.959495981Z
completed_at: 2026-02-26T07:24:08.959495981Z
transitions:
- status: complete
  at: 2026-02-26T07:24:08.959495981Z
---
# npm Publishing & Package Distribution

## Overview

Set up npm publishing for ClawDen packages, borrowing the proven platform-specific binary pattern from the leanspec project. The `@clawden` npm org is registered.

### Package Matrix

| npm Package | Source | Contents |
|---|---|---|
| `@clawden/sdk` | `sdk/` | TypeScript Skill SDK (`defineSkill`, types, utilities) |
| `clawden` | `crates/clawden-cli/` | Pre-built CLI binary (platform-specific via `optionalDependencies`) |
| `@clawden/cli-darwin-x64` | CI build | macOS x64 binary |
| `@clawden/cli-darwin-arm64` | CI build | macOS ARM64 binary |
| `@clawden/cli-linux-x64` | CI build | Linux x64 binary |
| `@clawden/cli-windows-x64` | CI build | Windows x64 binary |

The dashboard (`@clawden/dashboard`) remains **private** — it is not published.

## Design

### Pattern: leanspec-style Platform Binary Distribution

Borrowed from `~/projects/codervisor/leanspec`. The pattern (also used by esbuild, turbo, swc):

1. **Thin JS wrapper** (`npm/clawden/bin/clawden.js`) resolves the current platform and spawns the correct native binary
2. **Platform-specific optional deps** (`@clawden/cli-{platform}`) each contain a single pre-compiled Rust binary + postinstall script for chmod
3. **Main CLI package** (`clawden`) declares platform packages as `optionalDependencies` — npm/pnpm only installs the matching one
4. **CI cross-compiles** all targets, publishes platform packages first, waits for npm propagation, then publishes the umbrella

### Binary Resolution Order (in `bin/clawden.js`)
1. `target/debug/clawden-cli` — local dev (cargo build)
2. `target/release/clawden-cli` — local dev (cargo build --release)
3. `@clawden/cli-{platform}` — npm-installed platform package
4. `binaries/{platform}/clawden-cli` — local fallback

### `@clawden/sdk` — TypeScript Package
- Already has `tsup` build producing ESM + CJS + `.d.ts`
- Add `publishConfig`, `files`, `repository`, `license`, `description`, `keywords` to `package.json`
- Add `prepublishOnly` script running `pnpm build`

### Project Layout (new files)
```
clawden/
├── npm/
│   └── clawden/                    # Published as `clawden` on npm
│       ├── package.json            # bin, optionalDependencies, files
│       ├── bin/
│       │   └── clawden.js          # Platform detection + binary spawn
│       └── binaries/               # CI populates before publish
│           ├── darwin-x64/
│           │   ├── package.json    # @clawden/cli-darwin-x64
│           │   └── postinstall.js
│           ├── darwin-arm64/
│           ├── linux-x64/
│           └── windows-x64/
├── scripts/
│   ├── generate-platform-manifests.ts
│   ├── add-platform-deps.ts
│   ├── publish-platform-packages.ts
│   ├── publish-main-packages.ts
│   ├── prepare-publish.ts
│   ├── restore-packages.ts
│   ├── sync-versions.ts
│   ├── copy-platform-binaries.sh
│   └── validate-no-workspace-protocol.ts
└── .github/workflows/
    └── publish.yml
```

### Version Strategy
- All packages share a single version from root `package.json` (`0.1.0` initially)
- `sync-versions.ts` propagates root version → all packages + `Cargo.toml`
- Pre-releases use `-dev.<run_id>` suffix with `dev` dist-tag

### CI Publishing (GitHub Actions)
- Trigger: GitHub Release published OR manual `workflow_dispatch` (with dry_run / dev flags)
- Stage 1: Cross-compile Rust CLI for 4 targets (darwin-x64, darwin-arm64, linux-x64, windows-x64)
- Stage 2: Publish `@clawden/cli-{platform}` packages (parallel)
- Stage 3: Wait for npm propagation → build SDK → prepare-publish → publish `clawden` + `@clawden/sdk`
- Auth: `NPM_TOKEN` secret

## Plan

- [x] Create `npm/clawden/` wrapper package with `bin/clawden.js` launcher
- [x] Create platform binary directory scaffolds with postinstall.js
- [x] Create `scripts/generate-platform-manifests.ts`
- [x] Create `scripts/add-platform-deps.ts`
- [x] Create `scripts/publish-platform-packages.ts`
- [x] Create `scripts/publish-main-packages.ts`
- [x] Create `scripts/prepare-publish.ts` and `scripts/restore-packages.ts`
- [x] Create `scripts/sync-versions.ts`
- [x] Create `scripts/copy-platform-binaries.sh`
- [x] Create `scripts/validate-no-workspace-protocol.ts`
- [x] Create `.github/workflows/publish.yml`
- [x] Add publish metadata to `sdk/package.json`
- [x] Add `npm/clawden` to `pnpm-workspace.yaml`
- [x] Create root `package.json` (private, version source of truth)
- [x] Add `publish` recipes to justfile

## Test

- [x] `cd sdk && pnpm pack` produces a valid tarball with correct files
- [x] `cd npm/clawden && npm pack` includes bin/ and binaries/ correctly
- [x] `npm install clawden` on Linux x64 installs correct binary and `clawden --help` works
- [x] `npx clawden --help` works without global install
- [x] Platform binary wrapper falls back correctly when no npm package found (local dev)
- [x] CI publish workflow succeeds on tag push (dry-run first)
- [x] Dev prerelease publish works with `--dev` flag (via `workflow_dispatch` `inputs.dev`)

## Notes

- Pattern borrowed from `~/projects/codervisor/leanspec` which publishes 16 packages per release using the same strategy
- Alternative considered: single package with postinstall downloading the binary. Rejected because it requires network access at install time and breaks in locked-down CI environments.
- CI-only safety check in publish scripts prevents accidental local publishes (override with `--allow-local`)
- Platform packages use `postinstall.js` to set +x permissions since npm strips them