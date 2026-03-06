use anyhow::{Context, Result};
use clawden_core::{LifecycleManager, ProcessManager, RuntimeInstaller};
use std::collections::HashMap;
use std::process::{Command, Stdio};

use crate::cli::DockerCommand;
use crate::commands::run::{exec_run, RunOptions};
use crate::commands::up::{exec_up, UpOptions};
use crate::util::command_exists;

const DEFAULT_RUNTIME_IMAGE: &str = "ghcr.io/codervisor/openclaw:latest";

pub async fn exec_docker(
    command: DockerCommand,
    installer: &RuntimeInstaller,
    process_manager: &ProcessManager,
    manager: &mut LifecycleManager,
) -> Result<()> {
    ensure_docker_available()?;

    match command {
        DockerCommand::Run {
            runtime_and_args,
            channel,
            mut env_vars,
            env_file,
            provider,
            model,
            token,
            api_key,
            app_token,
            phone,
            allowed_users,
            system_prompt,
            tools,
            allow_missing_credentials,
            ports,
            volumes,
            rm,
            detach,
            restart,
            name,
            network,
            image,
        } => {
            let (runtime, args) = runtime_and_args.split_first().ok_or_else(|| {
                anyhow::anyhow!(
                    "missing runtime name: usage `clawden docker run <runtime> [runtime-args...]`"
                )
            })?;

            if !ports.is_empty() {
                env_vars.push(format!("CLAWDEN_PORT_MAP={}", ports.join(",")));
            }

            if !volumes.is_empty() {
                env_vars.push(format!("CLAWDEN_DOCKER_VOLUMES={}", volumes.join(";")));
            }
            env_vars.push(format!("CLAWDEN_DOCKER_RM={}", if rm { "1" } else { "0" }));
            if let Some(policy) = restart {
                env_vars.push(format!("CLAWDEN_DOCKER_RESTART={policy}"));
            }
            if let Some(container_name) = name {
                env_vars.push(format!("CLAWDEN_DOCKER_NAME={container_name}"));
            }
            if let Some(network_name) = network {
                env_vars.push(format!("CLAWDEN_DOCKER_NETWORK={network_name}"));
            }
            if !detach {
                eprintln!("Warning: adapter Docker run currently starts in background");
            }

            let mut env_guard = EnvVarGuard::default();
            if let Some(image_name) = image {
                env_guard.set("CLAWDEN_RUNTIME_IMAGE", &image_name);
            }

            exec_run(
                RunOptions {
                    runtime: runtime.to_string(),
                    channel,
                    env_vars,
                    env_file,
                    provider,
                    model,
                    token,
                    api_key,
                    app_token,
                    phone,
                    allowed_users,
                    system_prompt,
                    allow_missing_credentials,
                    tools,
                    detach,
                    extra_args: args.to_vec(),
                    force_docker: true,
                },
                installer,
                process_manager,
                manager,
            )
            .await
        }
        DockerCommand::Up {
            runtimes,
            env_vars,
            env_file,
            allow_missing_credentials,
            detach,
            no_log_prefix,
            timeout,
            build,
            force_recreate,
        } => {
            if build {
                eprintln!("Warning: --build is accepted but currently not applied by adapter orchestration");
            }
            if force_recreate {
                eprintln!(
                    "Warning: --force-recreate is accepted but currently not applied by adapter orchestration"
                );
            }

            exec_up(
                UpOptions {
                    runtimes,
                    env_vars,
                    env_file,
                    allow_missing_credentials,
                    detach,
                    no_log_prefix,
                    timeout,
                    force_docker: true,
                },
                installer,
                process_manager,
                manager,
            )
            .await
        }
        DockerCommand::Ps { all } => {
            let mut args = vec!["ps", "--filter", "label=clawden.managed=true"];
            if all {
                args.insert(1, "-a");
            }
            run_docker_passthrough(&args)
        }
        DockerCommand::Images { all, runtime } => {
            let mut args = vec!["images"];
            if all {
                args.push("-a");
            }
            let output = run_docker_capture(&args)?;
            if let Some(rt) = runtime {
                for line in output.lines() {
                    if line.to_ascii_lowercase().contains(&rt.to_ascii_lowercase()) {
                        println!("{line}");
                    }
                }
                return Ok(());
            }
            print!("{output}");
            Ok(())
        }
        DockerCommand::Pull { runtime, tag } => {
            let image = resolve_runtime_image(&runtime, tag.as_deref());
            run_docker_passthrough(&["pull", &image])
        }
        DockerCommand::Logs {
            runtime,
            follow,
            tail,
        } => {
            let container = resolve_container_id_or_name(&runtime)?;
            let mut owned = vec!["logs".to_string()];
            if follow {
                owned.push("-f".to_string());
            }
            if let Some(lines) = tail {
                owned.push("--tail".to_string());
                owned.push(lines.to_string());
            }
            owned.push(container);
            let refs = owned.iter().map(String::as_str).collect::<Vec<_>>();
            run_docker_passthrough(&refs)
        }
        DockerCommand::Exec {
            runtime,
            interactive,
            user,
            command,
        } => {
            let container = resolve_container_id_or_name(&runtime)?;
            let mut owned = vec!["exec".to_string()];
            if interactive {
                owned.push("-it".to_string());
            }
            if let Some(exec_user) = user {
                owned.push("--user".to_string());
                owned.push(exec_user);
            }
            owned.push(container);
            if command.is_empty() {
                owned.push("/bin/sh".to_string());
            } else {
                owned.extend(command);
            }
            let refs = owned.iter().map(String::as_str).collect::<Vec<_>>();
            run_docker_passthrough(&refs)
        }
        DockerCommand::Stop { runtime } => {
            let container = resolve_container_id_or_name(&runtime)?;
            run_docker_passthrough(&["stop", &container])
        }
        DockerCommand::Rm { runtime, force } => {
            let container = resolve_container_id_or_name(&runtime)?;
            if force {
                run_docker_passthrough(&["rm", "-f", &container])
            } else {
                run_docker_passthrough(&["rm", &container])
            }
        }
        DockerCommand::Build {
            runtime,
            tag,
            file,
            no_cache,
            context,
        } => {
            let mut owned = vec!["build".to_string()];
            if let Some(path) = file {
                owned.push("-f".to_string());
                owned.push(path);
            }
            if no_cache {
                owned.push("--no-cache".to_string());
            }
            let image_tag = match (runtime, tag) {
                (_, Some(custom_tag)) => custom_tag,
                (Some(rt), None) => resolve_runtime_image(&rt, Some("latest")),
                (None, None) => DEFAULT_RUNTIME_IMAGE.to_string(),
            };
            owned.push("-t".to_string());
            owned.push(image_tag);
            owned.push(context);
            let refs = owned.iter().map(String::as_str).collect::<Vec<_>>();
            run_docker_passthrough(&refs)
        }
    }
}

fn ensure_docker_available() -> Result<()> {
    if command_exists("docker") {
        return Ok(());
    }
    anyhow::bail!("docker is not available in PATH");
}

fn run_docker_passthrough(args: &[&str]) -> Result<()> {
    let status = Command::new("docker")
        .args(args)
        .status()
        .with_context(|| format!("failed to execute docker {}", args.join(" ")))?;
    if status.success() {
        return Ok(());
    }
    anyhow::bail!("docker {} failed", args.join(" "));
}

fn run_docker_capture(args: &[&str]) -> Result<String> {
    let output = Command::new("docker")
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .with_context(|| format!("failed to execute docker {}", args.join(" ")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("docker {} failed: {}", args.join(" "), stderr.trim());
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

fn resolve_runtime_image(runtime: &str, tag: Option<&str>) -> String {
    if let Ok(override_image) = std::env::var("CLAWDEN_RUNTIME_IMAGE") {
        if !override_image.trim().is_empty() {
            return override_image;
        }
    }

    let normalized = runtime.to_ascii_lowercase();
    let (repository, default_tag) = match normalized.as_str() {
        "openclaw" => ("openclaw", "latest"),
        "openclaw-browser" => ("openclaw", "browser"),
        "openclaw-computer" => ("openclaw", "computer"),
        "zeroclaw" => ("zeroclaw", "latest"),
        "zeroclaw-browser" => ("zeroclaw", "browser"),
        "zeroclaw-computer" => ("zeroclaw", "computer"),
        _ => (normalized.as_str(), "latest"),
    };
    let resolved_tag = tag.unwrap_or(default_tag);
    format!("ghcr.io/codervisor/{repository}:{resolved_tag}")
}

fn resolve_container_id_or_name(runtime_or_container: &str) -> Result<String> {
    if runtime_or_container.starts_with("clawden-") {
        return Ok(runtime_or_container.to_string());
    }

    let runtime_label = format!("label=clawden.runtime={runtime_or_container}");
    let output = run_docker_capture(&[
        "ps",
        "-a",
        "--filter",
        "label=clawden.managed=true",
        "--filter",
        &runtime_label,
        "--format",
        "{{.Names}}",
    ])?;
    let first = output
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(ToString::to_string);

    first.ok_or_else(|| {
        anyhow::anyhow!(
            "no managed container found for runtime '{runtime_or_container}'; pass a full container name"
        )
    })
}

#[derive(Default)]
struct EnvVarGuard {
    previous: HashMap<String, Option<String>>,
}

impl EnvVarGuard {
    fn set(&mut self, key: &str, value: &str) {
        if !self.previous.contains_key(key) {
            self.previous
                .insert(key.to_string(), std::env::var(key).ok());
        }
        std::env::set_var(key, value);
    }
}

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        for (key, prev) in self.previous.drain() {
            if let Some(value) = prev {
                std::env::set_var(key, value);
            } else {
                std::env::remove_var(key);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::resolve_runtime_image;

    #[test]
    fn resolve_runtime_image_uses_runtime_specific_repositories() {
        assert_eq!(
            resolve_runtime_image("openclaw", None),
            "ghcr.io/codervisor/openclaw:latest"
        );
        assert_eq!(
            resolve_runtime_image("zeroclaw", None),
            "ghcr.io/codervisor/zeroclaw:latest"
        );
    }

    #[test]
    fn resolve_runtime_image_maps_variant_names_to_alias_tags() {
        assert_eq!(
            resolve_runtime_image("openclaw-browser", None),
            "ghcr.io/codervisor/openclaw:browser"
        );
        assert_eq!(
            resolve_runtime_image("zeroclaw-computer", None),
            "ghcr.io/codervisor/zeroclaw:computer"
        );
    }
}
