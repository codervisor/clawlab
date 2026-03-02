use anyhow::Result;
use clawden_config::ClawDenYaml;
use clawden_core::{ExecutionMode, LifecycleManager, ProcessManager, RuntimeInstaller};

use crate::commands::InitOptions;
use crate::util::{
    append_audit_file, ensure_installed, env_no_docker_enabled, is_first_run_context,
    parse_runtime, prompt_yes_no,
};

pub struct RunOptions {
    pub runtime: Option<String>,
    pub channel: Vec<String>,
    pub tools: Option<String>,
    pub restart: Option<String>,
    pub no_docker: bool,
}

pub async fn exec_run(
    opts: RunOptions,
    installer: &RuntimeInstaller,
    process_manager: &ProcessManager,
    manager: &mut LifecycleManager,
) -> Result<()> {
    let RunOptions {
        runtime,
        channel,
        tools,
        restart,
        no_docker,
    } = opts;
    if runtime.is_none() && is_first_run_context(installer)? {
        let run_wizard = prompt_yes_no(
            "No project config found. Run setup wizard before starting a runtime?",
            true,
        )?;
        if run_wizard {
            super::exec_init(InitOptions {
                runtime: "zeroclaw".to_string(),
                multi: false,
                template: None,
                reconfigure: false,
                non_interactive: false,
                yes: false,
                force: false,
            })?;
        }
    }

    let rt = runtime.unwrap_or_else(|| "zeroclaw".to_string());
    let tools_list = tools
        .map(|t| {
            t.split(',')
                .map(|s| s.trim().to_string())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    // Load clawden.yaml if available for config/env enrichment
    let yaml_path = std::env::current_dir()?.join("clawden.yaml");
    let config = if yaml_path.exists() {
        let mut cfg = ClawDenYaml::from_file(&yaml_path).map_err(|e| anyhow::anyhow!("{}", e))?;
        let _ = cfg.resolve_env_vars();
        Some(cfg)
    } else {
        None
    };

    // Resolve channels: CLI args > clawden.yaml
    let resolved_channels = if !channel.is_empty() {
        channel
    } else if let Some(cfg) = config.as_ref() {
        cfg.channels.keys().cloned().collect()
    } else {
        Vec::new()
    };

    println!(
        "Running {} with channels {:?} and tools {:?}",
        rt, resolved_channels, tools_list
    );

    let mode = process_manager.resolve_mode(no_docker || env_no_docker_enabled());
    match mode {
        ExecutionMode::Docker => {
            let runtime = parse_runtime(&rt)?;
            let record = manager.register_agent(
                format!("{}-default", runtime.as_slug()),
                runtime,
                vec!["chat".to_string()],
            );
            manager
                .start_agent(&record.id)
                .await
                .map_err(anyhow::Error::msg)?;
            println!(
                "Started {} via core adapter path (docker available, server not required)",
                rt
            );
        }
        ExecutionMode::Direct | ExecutionMode::Auto => {
            let executable = ensure_installed(installer, &rt)?;

            let mut args = vec!["daemon".to_string()];
            if !resolved_channels.is_empty() {
                args.push(format!("--channels={}", resolved_channels.join(",")));
            }
            if !tools_list.is_empty() {
                args.push(format!("--tools={}", tools_list.join(",")));
            }
            if let Some(policy) = restart {
                args.push(format!("--restart={policy}"));
            }

            // Build env vars from clawden.yaml (channel creds + provider config)
            let env_vars = if let Some(cfg) = config.as_ref() {
                super::up::build_runtime_env_vars(cfg, &rt)?
            } else {
                Vec::new()
            };

            let info = process_manager.start_direct_with_env(&rt, &executable, &args, &env_vars)?;
            append_audit_file("runtime.start", &rt, "ok")?;
            println!(
                "Started {} in direct mode (pid {}, logs: {})",
                rt,
                info.pid,
                info.log_path.display()
            );
        }
    }

    Ok(())
}
