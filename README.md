# ClawDen

Developer experience layer for the xxxclaw ecosystem.
Install any claw runtime in one command, run it through one CLI, and manage updates without hand-rolling runtime-specific setup.

ClawDen combines three roles:
- `UX Shell`: one CLI and dashboard experience across runtimes
- `Runtime Manager`: install, pin, and update runtime versions
- `SDK Platform`: build cross-runtime skills with `@clawden/sdk`

## Quick start

### Guided onboarding

1. Initialize a project with the setup wizard:
	- `cargo run -p clawden-cli -- init`
2. For CI or scripts, use non-interactive mode:
	- `cargo run -p clawden-cli -- init --yes --runtime zeroclaw`
3. Use a starter template when needed:
	- `cargo run -p clawden-cli -- init --template telegram-bot --yes`
4. Validate local setup before startup:
	- `cargo run -p clawden-cli -- doctor`
5. Start runtimes:
	- `cargo run -p clawden-cli -- up`

### Transparent run model (`uv run` style)

`clawden run` forwards everything after the runtime name directly to that runtime:

- `cargo run -p clawden-cli -- run zeroclaw --verbose --model gpt-4`
- `cargo run -p clawden-cli -- run --channel telegram zeroclaw --verbose`

Rules:
- ClawDen flags go before runtime name: `--channel`, `--with`, `--provider`, `--model`, `--allowed-users`, `-d`
- Runtime flags go after runtime name and are passed through verbatim
- `clawden run zeroclaw --help` shows runtime help output (passthrough)
- Docker-specific execution uses `clawden docker run` (for `-p/--port`, `--rm`, `--restart`, `--network`, `--volume`, `--image`)

### Docker runtime image

ClawDen Docker images are runtime-specific — pick the one you need:

- OpenClaw: `docker run -e OPENAI_API_KEY=sk-... ghcr.io/codervisor/openclaw:latest`
- ZeroClaw: `docker run -e ANTHROPIC_API_KEY=sk-... ghcr.io/codervisor/zeroclaw:latest`
- Via CLI: `cargo run -p clawden-cli -- docker run openclaw`

Preferred immutable tags come from the pinned runtime versions in `docker/Dockerfile`, for example `ghcr.io/codervisor/openclaw:2026.3.2` and `ghcr.io/codervisor/zeroclaw:0.1.7-browser`.

Moving aliases remain available per runtime repository: `:latest`, `:browser`, and `:computer`.

Docker Compose (in `docker/`):

```bash
cp docker/.env.example docker/.env  # add your API key(s)
cd docker && docker compose up openclaw
```

### Config translation pipeline

Use one `clawden.yaml`; ClawDen translates it to runtime-native config automatically at run/start time.

- Config-dir runtimes (`zeroclaw`, `picoclaw`, `nullclaw`, `openfang`): generated under `~/.clawden/configs/<project_hash>/<runtime>/`
- Env-only runtimes (`openclaw`, `nanoclaw`): injected via `CLAWDEN_*` and runtime-specific environment variables

Validation happens before execution so missing provider keys or channel credentials fail with actionable errors.

Provider key management:
	- List configured providers: `cargo run -p clawden-cli -- providers`
	- Test provider credentials: `cargo run -p clawden-cli -- providers test`
	- Store a key in local encrypted vault: `cargo run -p clawden-cli -- providers set-key openai`

### Choose your path

- Hobbyist or student: `cargo run -p clawden-cli -- run zeroclaw`
- Solo developer: `cargo run -p clawden-cli -- install openclaw` then `cargo run -p clawden-cli -- run --channel telegram openclaw`
- Skill author: use `sdk/` and build with `pnpm --filter @clawden/sdk build`
- Team workflow: `cargo run -p clawden-cli -- up` and `cargo run -p clawden-cli -- dashboard`

### Rust backend and CLI

- Build: `cargo build`
- Test: `cargo test`
- Run server: `cargo run -p clawden-server`
- Run CLI: `cargo run -p clawden-cli -- --help`

### Direct install quickstart (no Docker)

1. Install one runtime:
	- `cargo run -p clawden-cli -- install zeroclaw`
	- `cargo run -p clawden-cli -- install openclaw`
2. Verify local prerequisites and installed runtimes:
	- `cargo run -p clawden-cli -- doctor`
	- `cargo run -p clawden-cli -- install --list`
3. Run directly on host:
	- `cargo run -p clawden-cli -- run zeroclaw`
4. Manage runtime processes:
	- `cargo run -p clawden-cli -- up`
	- `cargo run -p clawden-cli -- ps`
	- `cargo run -p clawden-cli -- logs zeroclaw --lines 50`
	- `cargo run -p clawden-cli -- stop`

Notes:
- Direct installs are stored under `~/.clawden/runtimes/`.
- Use `mode: direct` in `clawden.yaml` to force direct mode for `clawden up`.
- Use `cargo run -p clawden-cli -- docker run ...` or `cargo run -p clawden-cli -- docker up` to force Docker mode explicitly.
- To enable health checks in `clawden ps`, set runtime-specific health env vars such as `CLAWDEN_HEALTH_PORT_ZEROCLAW=8080` (or `CLAWDEN_HEALTH_URL_ZEROCLAW=http://127.0.0.1:8080/health`).

### Dashboard and SDK

- Install deps: `pnpm install`
- Dashboard dev: `pnpm --filter @clawden/dashboard dev`
- Dashboard test: `pnpm --filter @clawden/dashboard test`
- SDK build: `pnpm --filter @clawden/sdk build`
- SDK test: `pnpm --filter @clawden/sdk test`
