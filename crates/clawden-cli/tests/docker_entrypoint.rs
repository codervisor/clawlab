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

fn setup_fake_jq(bin_dir: &Path) {
    let script = r#"#!/usr/bin/env sh
set -eu
if [ "${1:-}" = "-Rsc" ]; then
  printf '[]\n'
  exit 0
fi
if [ "${1:-}" = "-n" ]; then
  printf '{"activated":[],"tools":{}}\n'
  exit 0
fi
printf '{}\n'
"#;
    write_executable(&bin_dir.join("jq"), script);
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

fn run_entrypoint(home: &Path, bin_dir: &Path, args: &[&str], runtime_env: Option<&str>) -> Output {
    let base_path = std::env::var("PATH").unwrap_or_default();
    let path = format!("{}:{}", bin_dir.display(), base_path);
    let mut command = Command::new("bash");
    command
        .arg(entrypoint_path())
        .current_dir(repo_root())
        .env("HOME", home)
        .env("PATH", path)
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
fn entrypoint_requires_runtime_or_wrapper_command() {
    let dir = temp_dir("missing-runtime");
    let home = dir.join("home");
    let bin_dir = dir.join("bin");
    fs::create_dir_all(&home).expect("home should be created");
    fs::create_dir_all(&bin_dir).expect("bin dir should be created");
    setup_fake_jq(&bin_dir);

    let output = run_entrypoint(&home, &bin_dir, &[], None);
    assert!(!output.status.success());

    let combined = combined_output(&output);
    assert!(combined.contains("ClawDen runtime image"));
    assert!(combined.contains("missing runtime name"));
    assert!(combined.contains("Supported runtimes:"));
}

#[test]
fn entrypoint_help_is_self_describing() {
    let dir = temp_dir("help");
    let home = dir.join("home");
    let bin_dir = dir.join("bin");
    fs::create_dir_all(&home).expect("home should be created");
    fs::create_dir_all(&bin_dir).expect("bin dir should be created");
    setup_fake_jq(&bin_dir);

    let output = run_entrypoint(&home, &bin_dir, &["--help"], None);
    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("ClawDen runtime image"));
    assert!(stdout.contains("docker run ghcr.io/codervisor/clawden-runtime:latest <runtime>"));
    assert!(stdout.contains("--list-runtimes"));
    assert!(!stdout.contains("[clawden] Starting runtime:"));
}

#[test]
fn entrypoint_lists_supported_runtimes() {
    let dir = temp_dir("list-runtimes");
    let home = dir.join("home");
    let bin_dir = dir.join("bin");
    fs::create_dir_all(&home).expect("home should be created");
    fs::create_dir_all(&bin_dir).expect("bin dir should be created");
    setup_fake_jq(&bin_dir);

    let output = run_entrypoint(&home, &bin_dir, &["--list-runtimes"], None);
    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("zeroclaw"));
    assert!(stdout.contains("picoclaw"));
    assert!(stdout.contains("openclaw"));
    assert!(stdout.contains("nanoclaw"));
    assert!(stdout.contains("openfang"));
}

#[test]
fn entrypoint_supports_positional_runtime_with_default_args() {
    let dir = temp_dir("positional-default");
    let home = dir.join("home");
    let bin_dir = dir.join("bin");
    fs::create_dir_all(&home).expect("home should be created");
    fs::create_dir_all(&bin_dir).expect("bin dir should be created");
    setup_fake_jq(&bin_dir);
    setup_fake_runtime(&home, "zeroclaw");

    let output = run_entrypoint(&home, &bin_dir, &["zeroclaw"], None);
    assert!(output.status.success());

    let combined = combined_output(&output);
    assert!(combined.contains("[clawden] Starting runtime: zeroclaw"));
    assert!(combined.contains("launcher runtime=zeroclaw args=daemon env_runtime=zeroclaw"));
}

#[test]
fn entrypoint_passes_positional_runtime_args_through() {
    let dir = temp_dir("positional-help");
    let home = dir.join("home");
    let bin_dir = dir.join("bin");
    fs::create_dir_all(&home).expect("home should be created");
    fs::create_dir_all(&bin_dir).expect("bin dir should be created");
    setup_fake_jq(&bin_dir);
    setup_fake_runtime(&home, "zeroclaw");

    let output = run_entrypoint(&home, &bin_dir, &["zeroclaw", "--help"], None);
    assert!(output.status.success());

    let combined = combined_output(&output);
    assert!(combined.contains("launcher runtime=zeroclaw args=--help env_runtime=zeroclaw"));
}

#[test]
fn entrypoint_preserves_env_driven_runtime_mode() {
    let dir = temp_dir("env-mode");
    let home = dir.join("home");
    let bin_dir = dir.join("bin");
    fs::create_dir_all(&home).expect("home should be created");
    fs::create_dir_all(&bin_dir).expect("bin dir should be created");
    setup_fake_jq(&bin_dir);
    setup_fake_runtime(&home, "zeroclaw");

    let output = run_entrypoint(&home, &bin_dir, &[], Some("zeroclaw"));
    assert!(output.status.success());

    let combined = combined_output(&output);
    assert!(combined.contains("[clawden] Starting runtime: zeroclaw"));
    assert!(combined.contains("launcher runtime=zeroclaw args=daemon env_runtime=zeroclaw"));
}

#[test]
fn entrypoint_rejects_unknown_runtime_with_usage() {
    let dir = temp_dir("invalid-runtime");
    let home = dir.join("home");
    let bin_dir = dir.join("bin");
    fs::create_dir_all(&home).expect("home should be created");
    fs::create_dir_all(&bin_dir).expect("bin dir should be created");
    setup_fake_jq(&bin_dir);

    let output = run_entrypoint(&home, &bin_dir, &["mysteryclaw"], None);
    assert!(!output.status.success());

    let combined = combined_output(&output);
    assert!(combined.contains("Unknown runtime 'mysteryclaw'"));
    assert!(combined.contains("Supported runtimes:"));
    assert!(combined.contains("zeroclaw, picoclaw, openclaw, nanoclaw, openfang"));
}
