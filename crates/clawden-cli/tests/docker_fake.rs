use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn temp_dir(name: &str) -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be after unix epoch")
        .as_nanos();
    let path = std::env::temp_dir().join(format!("clawden-{name}-{stamp}"));
    fs::create_dir_all(&path).expect("temp dir should be created");
    path
}

fn binary_path() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_clawden-cli"))
}

fn write_executable(path: &Path, content: &str) {
    fs::write(path, content).expect("script should be written");
    let mut perms = fs::metadata(path)
        .expect("metadata should be available")
        .permissions();
    perms.set_mode(0o755);
    fs::set_permissions(path, perms).expect("script should be executable");
}

fn setup_fake_docker(bin_dir: &Path, log_file: &Path) {
    let script = format!(
        r#"#!/usr/bin/env sh
set -eu
printf '%s\n' "$*" >> "{}"
case "$1" in
  --version)
    echo "Docker version 27.0.0, build fake"
    exit 0
    ;;
  rm)
    exit 0
    ;;
  run)
    echo "fake-container-id"
    exit 0
    ;;
  stop)
    exit 0
    ;;
  restart)
    exit 0
    ;;
  inspect)
    echo "true"
    exit 0
    ;;
  *)
    echo "unsupported fake docker command: $1" >&2
    exit 1
    ;;
esac
"#,
        log_file.display()
    );
    write_executable(&bin_dir.join("docker"), &script);
}

#[test]
fn up_docker_uses_passthrough_env_channels_and_tools() {
    let dir = temp_dir("docker-fake-up");
    let home = dir.join("home");
    let project = dir.join("project");
    let bin_dir = dir.join("bin");
    let docker_log = dir.join("docker.log");

    fs::create_dir_all(&home).expect("home should be created");
    fs::create_dir_all(&project).expect("project should be created");
    fs::create_dir_all(&bin_dir).expect("bin dir should be created");
    setup_fake_docker(&bin_dir, &docker_log);

    let yaml = r#"
runtime: zeroclaw
provider: openai
model: gpt-4o-mini
tools: [git, http]
channels:
  telegram:
    token: $TELEGRAM_BOT_TOKEN
"#;
    fs::write(project.join("clawden.yaml"), yaml).expect("yaml should be written");

    let base_path = std::env::var("PATH").unwrap_or_default();
    let path = format!("{}:{}", bin_dir.display(), base_path);

    let status = Command::new(binary_path())
        .current_dir(&project)
        .env("HOME", &home)
        .env("PATH", path)
        .env("OPENAI_API_KEY", "sk-up-test")
        .env("TELEGRAM_BOT_TOKEN", "tg-up-token")
        .args(["up", "--detach"])
        .status()
        .expect("up should run");
    assert!(status.success());

    let log = fs::read_to_string(&docker_log).expect("docker log should exist");
    assert!(log.contains("run -d --rm --name clawden-zeroclaw-zeroclaw-default"));
    assert!(log.contains("RUNTIME=zeroclaw"));
    assert!(log.contains("TOOLS=git,http"));
    assert!(log.contains("OPENAI_API_KEY=sk-up-test"));
    assert!(log.contains("CLAWDEN_LLM_MODEL=gpt-4o-mini"));
    assert!(log.contains("ZEROCLAW_LLM_MODEL=gpt-4o-mini"));
    assert!(log.contains("--channels=telegram"));
}

#[test]
fn run_docker_includes_cli_channel_and_tool_overrides() {
    let dir = temp_dir("docker-fake-run");
    let home = dir.join("home");
    let project = dir.join("project");
    let bin_dir = dir.join("bin");
    let docker_log = dir.join("docker.log");

    fs::create_dir_all(&home).expect("home should be created");
    fs::create_dir_all(&project).expect("project should be created");
    fs::create_dir_all(&bin_dir).expect("bin dir should be created");
    setup_fake_docker(&bin_dir, &docker_log);

    let yaml = r#"
runtime: zeroclaw
provider: openai
model: gpt-4o-mini
channels:
  telegram:
    token: $TELEGRAM_BOT_TOKEN
"#;
    fs::write(project.join("clawden.yaml"), yaml).expect("yaml should be written");

    let base_path = std::env::var("PATH").unwrap_or_default();
    let path = format!("{}:{}", bin_dir.display(), base_path);

    let status = Command::new(binary_path())
        .current_dir(&project)
        .env("HOME", &home)
        .env("PATH", path)
        .env("OPENAI_API_KEY", "sk-run-test")
        .env("TELEGRAM_BOT_TOKEN", "tg-run-token")
        .args([
            "run",
            "zeroclaw",
            "--channel",
            "discord",
            "--with",
            "git,http",
            "--detach",
        ])
        .status()
        .expect("run should execute");
    assert!(status.success());

    let log = fs::read_to_string(&docker_log).expect("docker log should exist");
    assert!(log.contains("run -d --rm --name clawden-zeroclaw-zeroclaw-default"));
    assert!(log.contains("RUNTIME=zeroclaw"));
    assert!(log.contains("TOOLS=git,http"));
    assert!(log.contains("OPENAI_API_KEY=sk-run-test"));
    assert!(log.contains("CLAWDEN_LLM_MODEL=gpt-4o-mini"));
    assert!(log.contains("ZEROCLAW_LLM_MODEL=gpt-4o-mini"));
    assert!(log.contains("--channels=discord"));
}

#[test]
fn up_fails_with_clear_error_when_env_reference_is_missing() {
    let dir = temp_dir("docker-fake-missing-env");
    let home = dir.join("home");
    let project = dir.join("project");

    fs::create_dir_all(&home).expect("home should be created");
    fs::create_dir_all(&project).expect("project should be created");

    let yaml = r#"
runtime: zeroclaw
channels:
  telegram:
    token: $TELEGRAM_BOT_TOKEN
"#;
    fs::write(project.join("clawden.yaml"), yaml).expect("yaml should be written");

    let output = Command::new(binary_path())
        .current_dir(&project)
        .env("HOME", &home)
        .env_remove("TELEGRAM_BOT_TOKEN")
        .args(["up", "--detach"])
        .output()
        .expect("up should execute");

    assert!(!output.status.success());
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        combined.contains("failed to resolve environment variables in clawden.yaml"),
        "combined output was: {combined}"
    );
    assert!(
        combined.contains("TELEGRAM_BOT_TOKEN"),
        "combined output was: {combined}"
    );
}
