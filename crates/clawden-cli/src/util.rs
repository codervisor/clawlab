use anyhow::Result;
use clawden_core::{ClawRuntime, RuntimeInstaller};
use std::fs::OpenOptions;
use std::io::{self, IsTerminal, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

pub fn parse_runtime(value: &str) -> Result<ClawRuntime> {
    ClawRuntime::from_str_loose(value).ok_or_else(|| anyhow::anyhow!("unknown runtime: {value}"))
}

pub fn parse_runtime_version(spec: &str) -> (String, Option<String>) {
    if let Some((runtime, version)) = spec.split_once('@') {
        (runtime.to_string(), Some(version.to_string()))
    } else {
        (spec.to_string(), None)
    }
}

pub fn command_exists(command: &str) -> bool {
    Command::new("which")
        .arg(command)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

pub fn env_no_docker_enabled() -> bool {
    std::env::var("CLAWDEN_NO_DOCKER")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

/// Return the executable for a runtime, auto-installing it if missing.
pub fn ensure_installed(installer: &RuntimeInstaller, runtime: &str) -> Result<PathBuf> {
    if let Some(exe) = installer.runtime_executable(runtime) {
        return Ok(exe);
    }
    println!("Runtime '{runtime}' not installed. Installing...");
    let installed = installer.install_runtime(runtime, None)?;
    println!("Installed {}@{}", installed.runtime, installed.version);
    Ok(installed.executable)
}

pub fn append_audit_file(action: &str, runtime: &str, outcome: &str) -> Result<()> {
    let home = std::env::var("HOME")?;
    let log_dir = PathBuf::from(home).join(".clawden").join("logs");
    std::fs::create_dir_all(&log_dir)?;
    let log_path = log_dir.join("audit.log");
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before UNIX_EPOCH")
        .as_millis();
    let line = format!("{now}\t{action}\t{runtime}\t{outcome}\n");

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)?;
    file.write_all(line.as_bytes())?;
    Ok(())
}

pub fn is_first_run_context(installer: &RuntimeInstaller) -> Result<bool> {
    let home = std::env::var("HOME")?;
    let clawden_home_exists = PathBuf::from(home).join(".clawden").exists();
    let cwd_has_yaml = std::env::current_dir()?.join("clawden.yaml").exists();
    let has_installed_runtimes = !installer.list_installed()?.is_empty();
    Ok(!clawden_home_exists && !cwd_has_yaml && !has_installed_runtimes)
}

pub fn prompt_yes_no(question: &str, default_yes: bool) -> Result<bool> {
    if !io::stdin().is_terminal() {
        return Ok(false);
    }
    let suffix = if default_yes { "[Y/n]" } else { "[y/N]" };
    print!("{question} {suffix} ");
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let normalized = input.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        return Ok(default_yes);
    }
    Ok(matches!(normalized.as_str(), "y" | "yes"))
}
