use anyhow::{bail, Context, Result};
use clawden_core::{AgentConfig, ClawRuntime, RuntimeConfig};
use std::process::{Command, Stdio};

const DEFAULT_IMAGE: &str = "ghcr.io/codervisor/clawden-runtime:latest";

pub fn runtime_config_values(runtime: &str, config: &AgentConfig) -> RuntimeConfig {
    RuntimeConfig {
        values: serde_json::json!({
            "runtime": runtime,
            "env_vars": config.env_vars,
            "channels": config.channels,
            "tools": config.tools,
        }),
    }
}

pub fn container_name(runtime: ClawRuntime, agent_name: &str) -> String {
    let mut normalized = String::new();
    for ch in agent_name.chars() {
        if ch.is_ascii_alphanumeric() {
            normalized.push(ch.to_ascii_lowercase());
        } else {
            normalized.push('-');
        }
    }
    let normalized = normalized.trim_matches('-');
    let normalized = if normalized.is_empty() {
        "default"
    } else {
        normalized
    };
    format!("clawden-{}-{}", runtime.as_slug(), normalized)
}

pub fn start_container(runtime: ClawRuntime, config: &AgentConfig) -> Result<String> {
    let default_name = container_name(runtime.clone(), &config.name);
    let name =
        docker_override(config, "CLAWDEN_DOCKER_NAME").unwrap_or_else(|| default_name.clone());
    if std::env::var("CLAWDEN_ADAPTER_DRY_RUN")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
    {
        return Ok(name);
    }

    ensure_docker_available()?;

    let image =
        std::env::var("CLAWDEN_RUNTIME_IMAGE").unwrap_or_else(|_| DEFAULT_IMAGE.to_string());

    // Best-effort cleanup in case a stale same-name container exists. Suppress
    // daemon noise when the container is absent or when a previous instance is
    // removed successfully.
    let _ = Command::new("docker")
        .args(["rm", "-f", &name])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();

    let args = build_run_args(runtime.clone(), config, &name, &image);

    let output = Command::new("docker")
        .args(&args)
        .output()
        .with_context(|| format!("failed to start docker runtime {}", runtime.as_slug()))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!(
            "docker run failed for {}: {}",
            runtime.as_slug(),
            stderr.trim()
        );
    }

    if !container_running(&name)? {
        let logs = container_logs(&name)?;
        if logs.is_empty() {
            bail!(
                "docker runtime {} exited immediately after start",
                runtime.as_slug()
            );
        }

        bail!(
            "docker runtime {} exited immediately after start:\n{}",
            runtime.as_slug(),
            logs
        );
    }

    Ok(name)
}

fn build_run_args(
    runtime: ClawRuntime,
    config: &AgentConfig,
    container_name: &str,
    image: &str,
) -> Vec<String> {
    let mut args = vec![
        "run".to_string(),
        "-d".to_string(),
        "--name".to_string(),
        container_name.to_string(),
        "--label".to_string(),
        "clawden.managed=true".to_string(),
        "--label".to_string(),
        format!("clawden.runtime={}", runtime.as_slug()),
        "-e".to_string(),
        format!("RUNTIME={}", runtime.as_slug()),
    ];

    if docker_bool_override(config, "CLAWDEN_DOCKER_RM").unwrap_or(true) {
        args.push("--rm".to_string());
    }

    if let Some(network) = docker_override(config, "CLAWDEN_DOCKER_NETWORK") {
        args.push("--network".to_string());
        args.push(network);
    }

    if let Some(restart) = docker_override(config, "CLAWDEN_DOCKER_RESTART") {
        args.push("--restart".to_string());
        args.push(restart);
    }

    if let Some(volumes) = docker_override(config, "CLAWDEN_DOCKER_VOLUMES") {
        for volume in volumes.split(';').map(str::trim).filter(|v| !v.is_empty()) {
            args.push("-v".to_string());
            args.push(volume.to_string());
        }
    }

    // Relaxed resource limits for managed execution — runtime-internal limits
    // are disabled via config injection; container-level limits provide the
    // outer boundary instead.
    args.extend([
        "--ulimit".to_string(),
        "nofile=65536:65536".to_string(),
        "--security-opt".to_string(),
        "seccomp=unconfined".to_string(),
    ]);

    // Signal to the runtime that ClawDen manages security.
    args.extend(["-e".to_string(), "CLAWDEN_MANAGED=1".to_string()]);

    if !config.tools.is_empty() {
        args.push("-e".to_string());
        args.push(format!("TOOLS={}", config.tools.join(",")));
    }

    for mapping in config
        .env_vars
        .iter()
        .filter(|(key, _)| key == "CLAWDEN_PORT_MAP")
        .flat_map(|(_, value)| value.split(','))
        .map(str::trim)
        .filter(|mapping| !mapping.is_empty())
    {
        args.push("-p".to_string());
        args.push(mapping.to_string());
    }

    for (key, value) in &config.env_vars {
        if key.starts_with("CLAWDEN_DOCKER_") {
            continue;
        }
        args.push("-e".to_string());
        args.push(format!("{}={}", key, value));
    }

    args.push(image.to_string());

    if !config.channels.is_empty() {
        args.push(format!("--channels={}", config.channels.join(",")));
    }

    args
}

fn docker_override(config: &AgentConfig, key: &str) -> Option<String> {
    config
        .env_vars
        .iter()
        .find(|(k, v)| k == key && !v.trim().is_empty())
        .map(|(_, v)| v.clone())
}

fn docker_bool_override(config: &AgentConfig, key: &str) -> Option<bool> {
    docker_override(config, key).map(|v| {
        let lower = v.to_ascii_lowercase();
        matches!(lower.as_str(), "1" | "true" | "yes" | "on")
    })
}

pub fn stop_container(container_id: &str) -> Result<()> {
    ensure_docker_available()?;
    let output = Command::new("docker")
        .args(["stop", container_id])
        .output()
        .context("failed to stop docker container")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("docker stop failed for {container_id}: {}", stderr.trim());
    }
    Ok(())
}

pub fn restart_container(container_id: &str) -> Result<()> {
    ensure_docker_available()?;
    let output = Command::new("docker")
        .args(["restart", container_id])
        .output()
        .context("failed to restart docker container")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!(
            "docker restart failed for {container_id}: {}",
            stderr.trim()
        );
    }
    Ok(())
}

pub fn container_running(container_id: &str) -> Result<bool> {
    ensure_docker_available()?;
    let output = Command::new("docker")
        .args(["inspect", "-f", "{{.State.Running}}", container_id])
        .output()
        .context("failed to inspect docker container")?;
    if !output.status.success() {
        return Ok(false);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(stdout.trim() == "true")
}

fn container_logs(container_id: &str) -> Result<String> {
    ensure_docker_available()?;
    let output = Command::new("docker")
        .args(["logs", "--tail", "50", container_id])
        .output()
        .context("failed to read docker container logs")?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = if stdout.trim().is_empty() {
        stderr.trim().to_string()
    } else if stderr.trim().is_empty() {
        stdout.trim().to_string()
    } else {
        format!("{}\n{}", stdout.trim(), stderr.trim())
    };

    Ok(combined)
}

fn ensure_docker_available() -> Result<()> {
    let output = Command::new("docker")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .context("failed to run docker --version")?;
    if !output.status.success() {
        bail!("docker CLI is not available");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{build_run_args, container_name, runtime_config_values};
    use clawden_core::{AgentConfig, ClawRuntime};

    #[test]
    fn container_name_is_sanitized() {
        let name = container_name(ClawRuntime::ZeroClaw, "My Agent@Dev");
        assert_eq!(name, "clawden-zeroclaw-my-agent-dev");
    }

    #[test]
    fn runtime_config_values_include_passthrough_fields() {
        let cfg = AgentConfig {
            name: "alpha".to_string(),
            runtime: ClawRuntime::ZeroClaw,
            model: None,
            env_vars: vec![("OPENAI_API_KEY".to_string(), "sk".to_string())],
            channels: vec!["telegram".to_string()],
            tools: vec!["git".to_string()],
        };

        let values = runtime_config_values("zeroclaw", &cfg).values;
        assert_eq!(values["channels"][0].as_str(), Some("telegram"));
        assert_eq!(values["tools"][0].as_str(), Some("git"));
        assert_eq!(values["env_vars"][0][0].as_str(), Some("OPENAI_API_KEY"));
    }

    #[test]
    fn build_run_args_includes_env_channels_and_tools() {
        let cfg = AgentConfig {
            name: "alpha".to_string(),
            runtime: ClawRuntime::ZeroClaw,
            model: None,
            env_vars: vec![("OPENAI_API_KEY".to_string(), "sk".to_string())],
            channels: vec!["telegram".to_string(), "discord".to_string()],
            tools: vec!["git".to_string(), "http".to_string()],
        };

        let args = build_run_args(
            ClawRuntime::ZeroClaw,
            &cfg,
            "clawden-zeroclaw-alpha",
            "ghcr.io/codervisor/clawden-runtime:latest",
        );

        assert!(args.contains(&"run".to_string()));
        assert!(args.contains(&"-d".to_string()));
        assert!(args.contains(&"RUNTIME=zeroclaw".to_string()));
        assert!(args.contains(&"TOOLS=git,http".to_string()));
        assert!(args.contains(&"OPENAI_API_KEY=sk".to_string()));
        assert!(args.contains(&"--channels=telegram,discord".to_string()));
        assert!(args.contains(&"clawden.managed=true".to_string()));
        assert!(args.contains(&"clawden.runtime=zeroclaw".to_string()));
    }

    #[test]
    fn build_run_args_applies_docker_overrides() {
        let cfg = AgentConfig {
            name: "alpha".to_string(),
            runtime: ClawRuntime::ZeroClaw,
            model: None,
            env_vars: vec![
                ("CLAWDEN_DOCKER_RM".to_string(), "0".to_string()),
                (
                    "CLAWDEN_DOCKER_RESTART".to_string(),
                    "unless-stopped".to_string(),
                ),
                (
                    "CLAWDEN_DOCKER_NETWORK".to_string(),
                    "clawden-net".to_string(),
                ),
                (
                    "CLAWDEN_DOCKER_VOLUMES".to_string(),
                    "/tmp/a:/a;/tmp/b:/b".to_string(),
                ),
            ],
            channels: vec![],
            tools: vec![],
        };

        let args = build_run_args(
            ClawRuntime::ZeroClaw,
            &cfg,
            "clawden-zeroclaw-alpha",
            "ghcr.io/codervisor/clawden-runtime:latest",
        );

        assert!(!args.contains(&"--rm".to_string()));
        assert!(args.contains(&"--restart".to_string()));
        assert!(args.contains(&"unless-stopped".to_string()));
        assert!(args.contains(&"--network".to_string()));
        assert!(args.contains(&"clawden-net".to_string()));
        assert!(args.contains(&"/tmp/a:/a".to_string()));
        assert!(args.contains(&"/tmp/b:/b".to_string()));
        assert!(
            !args
                .iter()
                .any(|entry| entry.starts_with("CLAWDEN_DOCKER_")),
            "internal docker override vars should not be forwarded into container env"
        );
    }
}
