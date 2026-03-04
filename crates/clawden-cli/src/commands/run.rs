use anyhow::Result;
use clawden_core::{
    validate_runtime_args, ExecutionMode, LifecycleManager, ProcessManager, RuntimeInstaller,
};
use std::time::Duration;

use crate::commands::config_gen::{generate_config_dir, inject_config_dir_arg};
use crate::commands::up::{
    build_runtime_env_vars, channels_for_runtime, load_config, pinned_version_for_runtime,
    render_log_line, tools_for_runtime, validate_direct_runtime_config, verify_runtime_startup,
};
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

    let config = load_config()?;

    // `clawden run` defaults to Direct mode (uv-run style transparent exec).
    // Only use Docker when clawden.yaml explicitly sets `mode: docker`.
    let config_mode_is_docker = config
        .as_ref()
        .and_then(|c| c.mode.as_deref())
        .is_some_and(|m| m.eq_ignore_ascii_case("docker"));
    let mode = if opts.no_docker || env_no_docker_enabled() {
        ExecutionMode::Direct
    } else if config_mode_is_docker {
        process_manager.resolve_mode(false)
    } else {
        ExecutionMode::Direct
    };
    let env_vars = if let Some(cfg) = config.as_ref() {
        build_runtime_env_vars(cfg, &opts.runtime)?
    } else {
        Vec::new()
    };

    let default_channels = if let Some(cfg) = config.as_ref() {
        channels_for_runtime(cfg, &opts.runtime)
    } else {
        Vec::new()
    };

    let default_tools = if let Some(cfg) = config.as_ref() {
        tools_for_runtime(cfg, &opts.runtime)
    } else {
        Vec::new()
    };

    let resolved_channels = if !opts.channel.is_empty() {
        opts.channel.clone()
    } else {
        default_channels
    };
    let effective_tools = if !tools_list.is_empty() {
        tools_list.clone()
    } else {
        default_tools.clone()
    };

    let pinned_version = config
        .as_ref()
        .and_then(|cfg| pinned_version_for_runtime(cfg, &opts.runtime));

    match mode {
        ExecutionMode::Docker => {
            if !opts.extra_args.is_empty() {
                eprintln!(
                    "Warning: extra runtime args ({}) are ignored in Docker mode. \
                     Use --no-docker or set mode: direct in clawden.yaml to pass args through.",
                    opts.extra_args.join(" "),
                );
            }
            let runtime = parse_runtime(&opts.runtime)?;
            let record = manager.register_agent_with_config(
                format!("{}-default", runtime.as_slug()),
                runtime.clone(),
                vec!["chat".to_string()],
                clawden_core::AgentConfig {
                    name: format!("{}-default", runtime.as_slug()),
                    runtime,
                    model: None,
                    env_vars: env_vars.clone(),
                    channels: resolved_channels.clone(),
                    tools: effective_tools,
                },
            );
            manager
                .start_agent(&record.id)
                .await
                .map_err(anyhow::Error::msg)?;
            println!("Started {} via Docker", opts.runtime);
            return Ok(());
        }
        ExecutionMode::Direct | ExecutionMode::Auto => {}
    }

    let current_project_hash = project_hash()?;
    let installed = ensure_installed_runtime(installer, &opts.runtime, pinned_version)?;

    let mut args = installed.start_args.clone();
    if let Some(policy) = &opts.restart {
        args.push(format!("--restart={policy}"));
    }
    if let Some(cfg) = config.as_ref() {
        if let Some(config_dir) = generate_config_dir(cfg, &opts.runtime, &current_project_hash)? {
            inject_config_dir_arg(&opts.runtime, &mut args, &config_dir);
        }
    }
    args.extend(opts.extra_args.clone());

    let unsupported = validate_runtime_args(&opts.runtime, &args);
    if !unsupported.is_empty() {
        eprintln!(
            "Warning: {} does not accept these flags: {}. They will be passed anyway since they were explicitly requested.",
            opts.runtime,
            unsupported.join(", "),
        );
    }

    // Channel and tool lists are passed via env vars — runtimes
    // do NOT accept --channels / --tools CLI flags.
    let mut combined_env = env_vars;
    if let Some(cfg) = config.as_ref() {
        validate_direct_runtime_config(cfg, &opts.runtime, &combined_env, &resolved_channels)?;
    }
    if !resolved_channels.is_empty() {
        combined_env.push(("CLAWDEN_CHANNELS".to_string(), resolved_channels.join(",")));
    }
    if !effective_tools.is_empty() {
        combined_env.push(("CLAWDEN_TOOLS".to_string(), effective_tools.join(",")));
    }

    let info = process_manager.start_direct_with_env_and_project(
        &opts.runtime,
        &installed.executable,
        &args,
        &combined_env,
        Some(current_project_hash),
    )?;
    verify_runtime_startup(process_manager, &opts.runtime, &info)?;
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
    let stream = process_manager.stream_logs(std::slice::from_ref(&opts.runtime))?;
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
