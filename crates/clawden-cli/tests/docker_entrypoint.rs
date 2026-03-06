use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

fn temp_dir(name: &str) -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be after unix epoch")
        .as_nanos();
    let path = std::env::temp_dir().join(format!("clawden-entrypoint-{name}-{stamp}"));
    fs::create_dir_all(&path).expect("temp dir should be created");
    path
}

fn write_executable(path: &Path, content: &str) {
    fs::write(path, content).expect("script should be written");
    let mut perms = fs::metadata(path)
        .expect("metadata should be available")
        .permissions();
    perms.set_mode(0o755);
    fs::set_permissions(path, perms).expect("script should be executable");
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crate dir should have parent")
        .parent()
        .expect("repo root should exist")
        .to_path_buf()
}

fn entrypoint_path() -> PathBuf {
    repo_root().join("docker/entrypoint.sh")
}

fn setup_fake_runtime(home: &Path, runtime: &str) {
    let launcher = home
        .join(".clawden/runtimes")
        .join(runtime)
        .join("current")
        .join(runtime);
    fs::create_dir_all(launcher.parent().expect("launcher parent should exist"))
        .expect("runtime dir should be created");
    let script = r#"#!/usr/bin/env sh
set -eu
printf 'launcher runtime=%s args=%s env_runtime=%s\n' "$(basename "$0")" "$*" "${RUNTIME:-}"
"#;
    write_executable(&launcher, script);
}

fn run_entrypoint(home: &Path, args: &[&str], runtime_env: Option<&str>) -> Output {
    let mut command = Command::new("bash");
    command
        .arg(entrypoint_path())
        .current_dir(repo_root())
        .env("HOME", home)
        .args(args);
    if let Some(runtime) = runtime_env {
        command.env("RUNTIME", runtime);
    } else {
        command.env_remove("RUNTIME");
    }
    command.output().expect("entrypoint should execute")
}

fn combined_output(output: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

#[test]
fn entrypoint_requires_runtime() {
    let dir = temp_dir("missing-runtime");
    let home = dir.join("home");
    fs::create_dir_all(&home).expect("home should be created");

    let output = run_entrypoint(&home, &[], None);
    assert!(!output.status.success());

    let combined = combined_output(&output);
    assert!(combined.contains("RUNTIME not set"));
    assert!(combined.contains("openclaw:latest"));
    assert!(combined.contains("zeroclaw:latest"));
}

#[test]
fn entrypoint_help_is_self_describing() {
    let dir = temp_dir("help");
    let home = dir.join("home");
    fs::create_dir_all(&home).expect("home should be created");

    let output = run_entrypoint(&home, &["--help"], None);
    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("ClawDen Docker Image"));
    assert!(stdout.contains("ghcr.io/codervisor/openclaw:latest"));
    assert!(stdout.contains("ghcr.io/codervisor/zeroclaw:latest"));
}

#[test]
fn entrypoint_supports_positional_runtime_with_default_args() {
    let dir = temp_dir("positional-default");
    let home = dir.join("home");
    fs::create_dir_all(&home).expect("home should be created");
    setup_fake_runtime(&home, "zeroclaw");

    let output = run_entrypoint(&home, &["zeroclaw"], None);
    assert!(output.status.success());

    let combined = combined_output(&output);
    assert!(combined.contains("Starting ZeroClaw daemon"));
    assert!(combined.contains("launcher runtime=zeroclaw"));
    assert!(combined.contains("daemon --config-dir"));
}

#[test]
fn entrypoint_passes_positional_runtime_args_through() {
    let dir = temp_dir("positional-help");
    let home = dir.join("home");
    fs::create_dir_all(&home).expect("home should be created");
    setup_fake_runtime(&home, "zeroclaw");

    let output = run_entrypoint(&home, &["zeroclaw", "--help"], None);
    assert!(output.status.success());

    let combined = combined_output(&output);
    assert!(combined.contains("launcher runtime=zeroclaw args=--help env_runtime=zeroclaw"));
}

#[test]
fn entrypoint_preserves_env_driven_runtime_mode() {
    let dir = temp_dir("env-mode");
    let home = dir.join("home");
    fs::create_dir_all(&home).expect("home should be created");
    setup_fake_runtime(&home, "zeroclaw");

    let output = run_entrypoint(&home, &[], Some("zeroclaw"));
    assert!(output.status.success());

    let combined = combined_output(&output);
    assert!(combined.contains("Starting ZeroClaw daemon"));
    assert!(combined.contains("launcher runtime=zeroclaw"));
}

#[test]
fn entrypoint_openclaw_default_args() {
    let dir = temp_dir("openclaw-default");
    let home = dir.join("home");
    fs::create_dir_all(&home).expect("home should be created");
    setup_fake_runtime(&home, "openclaw");

    let output = run_entrypoint(&home, &[], Some("openclaw"));
    assert!(output.status.success());

    let combined = combined_output(&output);
    assert!(combined.contains("Starting OpenClaw gateway"));
    assert!(combined.contains("launcher runtime=openclaw args=gateway --allow-unconfigured"));
}

#[test]
fn entrypoint_rejects_unknown_runtime() {
    let dir = temp_dir("invalid-runtime");
    let home = dir.join("home");
    fs::create_dir_all(&home).expect("home should be created");

    let output = run_entrypoint(&home, &["mysteryclaw"], None);
    assert!(!output.status.success());

    let combined = combined_output(&output);
    assert!(combined.contains("Unknown runtime"));
    assert!(combined.contains("openclaw"));
    assert!(combined.contains("zeroclaw"));
}

#[test]
fn entrypoint_zeroclaw_generates_config() {
    let dir = temp_dir("zeroclaw-config");
    let home = dir.join("home");
    fs::create_dir_all(&home).expect("home should be created");
    setup_fake_runtime(&home, "zeroclaw");

    let output = run_entrypoint(&home, &[], Some("zeroclaw"));
    assert!(output.status.success());

    let config_path = home.join(".clawden/zeroclaw/config.toml");
    assert!(
        config_path.exists(),
        "ZeroClaw config should be auto-generated"
    );

    let config_content = fs::read_to_string(&config_path).expect("config should be readable");
    assert!(config_content.contains("[channels_config]"));
    assert!(config_content.contains("cli = true"));
}
