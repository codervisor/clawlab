use anyhow::{bail, Context, Result};
use clawden_core::{AgentConfig, ClawRuntime, RuntimeConfig};
use std::collections::HashMap;
use std::process::Command;

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
    let name = container_name(runtime.clone(), &config.name);
    if std::env::var("CLAWDEN_ADAPTER_DRY_RUN")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
    {
        return Ok(name);
    }

    ensure_docker_available()?;

    let image =
        std::env::var("CLAWDEN_RUNTIME_IMAGE").unwrap_or_else(|_| DEFAULT_IMAGE.to_string());

    // Best-effort cleanup in case a stale same-name container exists.
    let _ = Command::new("docker").args(["rm", "-f", &name]).status();

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
        "--rm".to_string(),
        "--name".to_string(),
        container_name.to_string(),
        "-e".to_string(),
        format!("RUNTIME={}", runtime.as_slug()),
    ];

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
        args.push("-e".to_string());
        args.push(format!("{}={}", key, value));
    }

    args.push(image.to_string());

    if !config.channels.is_empty() {
        args.push(format!("--channels={}", config.channels.join(",")));
    }

    args
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

pub fn set_stored_config(
    store: &std::sync::Mutex<HashMap<String, RuntimeConfig>>,
    handle_id: &str,
    config: RuntimeConfig,
) {
    if let Ok(mut guard) = store.lock() {
        guard.insert(handle_id.to_string(), config);
    }
}

pub fn get_stored_config(
    store: &std::sync::Mutex<HashMap<String, RuntimeConfig>>,
    handle_id: &str,
) -> Option<RuntimeConfig> {
    store
        .lock()
        .ok()
        .and_then(|guard| guard.get(handle_id).cloned())
}

pub fn remove_stored_config(
    store: &std::sync::Mutex<HashMap<String, RuntimeConfig>>,
    handle_id: &str,
) {
    if let Ok(mut guard) = store.lock() {
        guard.remove(handle_id);
    }
}

fn ensure_docker_available() -> Result<()> {
    let status = Command::new("docker")
        .arg("--version")
        .status()
        .context("failed to run docker --version")?;
    if !status.success() {
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
    }
}
