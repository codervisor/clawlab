use anyhow::Result;
use clawden_config::ClawDenYaml;
use clawden_core::{ExecutionMode, LifecycleManager, ProcessManager, RuntimeInstaller};

use crate::util::{append_audit_file, ensure_installed, env_no_docker_enabled, parse_runtime};

pub async fn exec_up(
    runtimes: Vec<String>,
    no_docker: bool,
    installer: &RuntimeInstaller,
    process_manager: &ProcessManager,
    manager: &mut LifecycleManager,
) -> Result<()> {
    let mode = process_manager.resolve_mode(no_docker || env_no_docker_enabled());

    // Determine target runtimes: CLI args > clawden.yaml > installed runtimes
    let target_runtimes = if !runtimes.is_empty() {
        runtimes
    } else {
        let yaml_path = std::env::current_dir()?.join("clawden.yaml");
        if yaml_path.exists() {
            let config =
                ClawDenYaml::from_file(&yaml_path).map_err(|e| anyhow::anyhow!("{}", e))?;
            if let Err(errs) = config.validate() {
                anyhow::bail!("clawden.yaml validation failed:\n{}", errs.join("\n"));
            }
            let from_config = runtimes_from_config(&config);
            if from_config.is_empty() {
                anyhow::bail!("clawden.yaml does not define any runtimes");
            }
            println!(
                "Using runtimes from clawden.yaml: {}",
                from_config.join(", ")
            );
            from_config
        } else {
            installer
                .list_installed()?
                .into_iter()
                .map(|row| row.runtime)
                .collect::<Vec<_>>()
        }
    };

    if target_runtimes.is_empty() {
        println!("No runtimes to start. Create a clawden.yaml with 'clawden init' or install one with 'clawden install zeroclaw'");
        return Ok(());
    }

    for runtime in target_runtimes {
        match mode {
            ExecutionMode::Docker => {
                let rt = parse_runtime(&runtime)?;
                let record = manager.register_agent(
                    format!("{}-default", rt.as_slug()),
                    rt,
                    vec!["chat".to_string()],
                );
                manager
                    .start_agent(&record.id)
                    .await
                    .map_err(anyhow::Error::msg)?;
                append_audit_file("runtime.start", &runtime, "ok")?;
                println!("Started {runtime} via adapter (docker mode)");
            }
            ExecutionMode::Direct | ExecutionMode::Auto => {
                let executable = ensure_installed(installer, &runtime)?;
                let info = process_manager.start_direct(&runtime, &executable, &[])?;
                append_audit_file("runtime.start", &runtime, "ok")?;
                println!("Started {runtime} (pid {})", info.pid);
            }
        }
    }

    Ok(())
}

/// Extract runtime names from a parsed clawden.yaml config.
fn runtimes_from_config(config: &ClawDenYaml) -> Vec<String> {
    if let Some(rt) = &config.runtime {
        vec![rt.clone()]
    } else {
        config.runtimes.iter().map(|r| r.name.clone()).collect()
    }
}
