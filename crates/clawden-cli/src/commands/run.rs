use anyhow::Result;
use clawden_core::{ExecutionMode, LifecycleManager, ProcessManager, RuntimeInstaller};

use crate::commands::InitOptions;
use crate::util::{
    append_audit_file, ensure_installed, env_no_docker_enabled, is_first_run_context, parse_runtime,
    prompt_yes_no,
};

pub async fn exec_run(
    runtime: Option<String>,
    channel: Vec<String>,
    tools: Option<String>,
    restart: Option<String>,
    no_docker: bool,
    installer: &RuntimeInstaller,
    process_manager: &ProcessManager,
    manager: &mut LifecycleManager,
) -> Result<()> {
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
    println!(
        "Running {} with channels {:?} and tools {:?}",
        rt, channel, tools_list
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

            let mut args = Vec::new();
            if !channel.is_empty() {
                args.push(format!("--channels={}", channel.join(",")));
            }
            if let Some(policy) = restart {
                args.push(format!("--restart={policy}"));
            }

            let info = process_manager.start_direct(&rt, &executable, &args)?;
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
