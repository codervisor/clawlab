use anyhow::Result;
use clawden_core::{ExecutionMode, LifecycleManager, ProcessManager, RuntimeInstaller};
use std::time::Duration;

use crate::commands::up::{build_runtime_env_vars, load_config, render_log_line};
use crate::util::{
    append_audit_file, ensure_installed_runtime, env_no_docker_enabled, parse_runtime, project_hash,
};

pub struct RunOptions {
    pub runtime: String,
    pub channel: Vec<String>,
    pub tools: Option<String>,
    pub restart: Option<String>,
    pub detach: bool,
    pub rm: bool,
    pub extra_args: Vec<String>,
    pub no_docker: bool,
}

pub async fn exec_run(
    opts: RunOptions,
    installer: &RuntimeInstaller,
    process_manager: &ProcessManager,
    manager: &mut LifecycleManager,
) -> Result<()> {
    let tools_list = opts
        .tools
        .clone()
        .map(|t| {
            t.split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let mode = process_manager.resolve_mode(opts.no_docker || env_no_docker_enabled());
    match mode {
        ExecutionMode::Docker => {
            let runtime = parse_runtime(&opts.runtime)?;
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
                opts.runtime
            );
            return Ok(());
        }
        ExecutionMode::Direct | ExecutionMode::Auto => {}
    }

    let config = load_config()?;
    let installed = ensure_installed_runtime(installer, &opts.runtime)?;

    let resolved_channels = if !opts.channel.is_empty() {
        opts.channel.clone()
    } else if let Some(cfg) = config.as_ref() {
        cfg.channels.keys().cloned().collect()
    } else {
        Vec::new()
    };

    let mut args = installed.start_args.clone();
    if !resolved_channels.is_empty() {
        args.push(format!("--channels={}", resolved_channels.join(",")));
    }
    if !tools_list.is_empty() {
        args.push(format!("--tools={}", tools_list.join(",")));
    }
    if let Some(policy) = &opts.restart {
        args.push(format!("--restart={policy}"));
    }
    args.extend(opts.extra_args.clone());

    let env_vars = if let Some(cfg) = config.as_ref() {
        build_runtime_env_vars(cfg, &opts.runtime)?
    } else {
        Vec::new()
    };

    let info = process_manager.start_direct_with_env_and_project(
        &opts.runtime,
        &installed.executable,
        &args,
        &env_vars,
        Some(project_hash()?),
    )?;
    append_audit_file("runtime.start", &opts.runtime, "ok")?;

    if opts.detach {
        println!(
            "Started {} in direct mode (pid {}, logs: {})",
            opts.runtime,
            info.pid,
            info.log_path.display()
        );
        return Ok(());
    }

    println!(
        "Running {} in foreground. Press Ctrl+C to stop.",
        opts.runtime
    );
    let stream = process_manager.stream_logs(&[opts.runtime.clone()])?;
    let mut tick = tokio::time::interval(Duration::from_millis(150));
    let ctrl_c = tokio::signal::ctrl_c();
    tokio::pin!(ctrl_c);

    loop {
        tokio::select! {
            _ = &mut ctrl_c => {
                let outcome = process_manager.stop_with_timeout(&opts.runtime, 10)?;
                if outcome.forced {
                    append_audit_file("runtime.force_kill", &opts.runtime, "ok")?;
                }
                append_audit_file("runtime.stop", &opts.runtime, "ok")?;
                break;
            }
            _ = tick.tick() => {
                for line in stream.drain() {
                    println!("{}", render_log_line(&line.runtime, &line.text, true, None));
                }

                let status = process_manager.list_statuses()?.into_iter().find(|s| s.runtime == opts.runtime);
                if !status.map(|s| s.running).unwrap_or(false) {
                    break;
                }
            }
        }
    }

    if opts.rm {
        let _ = process_manager.stop_with_timeout(&opts.runtime, 1)?;
    }

    Ok(())
}
