use anyhow::Result;
use clawden_config::{
    ChannelCredentialMapper, ClawDenYaml, LlmProvider, ProviderEntryYaml, ProviderRefYaml,
};
use clawden_core::{
    runtime_default_start_args, AgentState, ExecutionMode, LifecycleManager, ProcessInfo,
    ProcessManager, RuntimeInstaller,
};
use std::collections::HashMap;
use std::time::Duration;
use tracing::{debug, warn};

use crate::commands::config_gen::{generate_config_dir, inject_config_dir_arg, state_dir_env_vars};
use crate::commands::InitOptions;
use crate::util::{
    append_audit_file, ensure_installed_runtime, get_provider_key_from_vault, is_first_run_context,
    parse_runtime, project_hash, prompt_yes_no,
};

pub struct UpOptions {
    pub runtimes: Vec<String>,
    pub env_vars: Vec<String>,
    pub env_file: Option<String>,
    pub allow_missing_credentials: bool,
    pub detach: bool,
    pub no_log_prefix: bool,
    pub timeout: u64,
    pub force_docker: bool,
}

pub async fn exec_up(
    opts: UpOptions,
    installer: &RuntimeInstaller,
    process_manager: &ProcessManager,
    manager: &mut LifecycleManager,
) -> Result<()> {
    if opts.runtimes.is_empty() && is_first_run_context(installer)? {
        let run_wizard = prompt_yes_no(
            "No clawden.yaml found and no installed runtimes. Run setup wizard now?",
            true,
        )?;
        if run_wizard {
            super::exec_init(InitOptions {
                runtime: "zeroclaw".to_string(),
                multi: false,
                template: None,
                non_interactive: false,
                yes: false,
                force: false,
            })?;
        } else {
            println!("Setup skipped.");
            return Ok(());
        }
    }

    let config = load_config_with_env_file(opts.env_file.as_deref())?;
    let config_mode_is_direct = config
        .as_ref()
        .and_then(|c| c.mode.as_deref())
        .is_some_and(|m| m.eq_ignore_ascii_case("direct"));
    let mode = if opts.force_docker {
        process_manager.resolve_mode(false)
    } else {
        process_manager.resolve_mode(config_mode_is_direct)
    };
    let target_runtimes =
        resolve_target_runtimes(opts.runtimes.clone(), config.as_ref(), installer)?;

    if target_runtimes.is_empty() {
        println!("No runtimes to start. Create a clawden.yaml with 'clawden init' or install one with 'clawden install zeroclaw'");
        return Ok(());
    }
    let current_project_hash = project_hash()?;

    let mut started_runtimes = Vec::new();

    for runtime in target_runtimes {
        let env_vars = if let Some(cfg) = config.as_ref() {
            build_runtime_env_vars(cfg, &runtime)?
        } else {
            Vec::new()
        };
        let env_overrides = parse_env_overrides(&opts.env_vars)?;
        if !env_overrides.is_empty() {
            let keys = env_overrides
                .iter()
                .map(|(k, _)| k.as_str())
                .collect::<Vec<_>>()
                .join(",");
            let _ = append_audit_file("runtime.env_override", &runtime, &keys);
        }

        let channels = if let Some(cfg) = config.as_ref() {
            channels_for_runtime(cfg, &runtime)
        } else {
            Vec::new()
        };

        let tools = if let Some(cfg) = config.as_ref() {
            tools_for_runtime(cfg, &runtime)
        } else {
            Vec::new()
        };

        let pinned_version = config
            .as_ref()
            .and_then(|cfg| pinned_version_for_runtime(cfg, &runtime));

        match mode {
            ExecutionMode::Docker => {
                let rt = parse_runtime(&runtime)?;
                let mut docker_env = env_vars.clone();
                for (key, value) in &env_overrides {
                    docker_env.retain(|(k, _)| k != key);
                    docker_env.push((key.clone(), value.clone()));
                }
                let record = manager.register_agent_with_config(
                    format!("{}-default", rt.as_slug()),
                    rt.clone(),
                    vec!["chat".to_string()],
                    clawden_core::AgentConfig {
                        name: format!("{}-default", rt.as_slug()),
                        runtime: rt,
                        model: None,
                        env_vars: docker_env,
                        channels: channels.clone(),
                        tools: tools.clone(),
                    },
                );
                manager
                    .start_agent(&record.id)
                    .await
                    .map_err(anyhow::Error::msg)?;
                append_audit_file("runtime.start", &runtime, "ok")?;
                println!("Started {runtime} via adapter (docker mode)");
                started_runtimes.push(runtime.clone());
            }
            ExecutionMode::Direct | ExecutionMode::Auto => {
                if let Some(cfg) = config.as_ref() {
                    if !opts.allow_missing_credentials {
                        validate_direct_runtime_config(cfg, &runtime, &env_vars, &channels)?;
                    } else {
                        warn!(
                            "missing credential checks are skipped (--allow-missing-credentials)"
                        );
                    }
                }
                let installed = ensure_installed_runtime(installer, &runtime, pinned_version)?;

                let mut args = runtime_default_start_args(&runtime)
                    .iter()
                    .map(|arg| (*arg).to_string())
                    .collect::<Vec<_>>();
                if let Some(cfg) = config.as_ref() {
                    if let Some(config_dir) = generate_config_dir(
                        cfg,
                        &runtime,
                        &current_project_hash,
                        Some(&installed.executable),
                    )? {
                        inject_config_dir_arg(&runtime, &mut args, &config_dir);
                    }
                }

                // Channel and tool lists are passed via env vars — runtimes
                // do NOT accept --channels / --tools CLI flags.
                let mut combined_env = env_vars.clone();
                combined_env.extend(state_dir_env_vars(&runtime, &current_project_hash)?);
                if !channels.is_empty() {
                    combined_env.push(("CLAWDEN_CHANNELS".to_string(), channels.join(",")));
                }
                if !tools.is_empty() {
                    combined_env.push(("CLAWDEN_TOOLS".to_string(), tools.join(",")));
                }
                for (key, value) in &env_overrides {
                    combined_env.retain(|(k, _)| k != key);
                    combined_env.push((key.clone(), value.clone()));
                }

                let info = process_manager.start_direct_with_env_and_project(
                    &runtime,
                    &installed.executable,
                    &args,
                    &combined_env,
                    Some(current_project_hash.clone()),
                )?;
                verify_runtime_startup(process_manager, &runtime, &info)?;
                append_audit_file("runtime.start", &runtime, "ok")?;
                println!("Started {runtime} (pid {})", info.pid);
                started_runtimes.push(runtime.clone());
            }
        }
    }

    if opts.detach {
        print_status_table(process_manager)?;
        return Ok(());
    }

    if started_runtimes.is_empty() {
        return Ok(());
    }

    println!("Attaching logs. Press Ctrl+C to stop.");
    let stream = process_manager.stream_logs(&started_runtimes)?;
    let mut tick = tokio::time::interval(Duration::from_millis(150));
    let ctrl_c = tokio::signal::ctrl_c();
    tokio::pin!(ctrl_c);

    loop {
        tokio::select! {
            _ = &mut ctrl_c => {
                println!("Gracefully stopping...");
                let to_stop = started_runtimes.clone();
                let timeout = opts.timeout;
                let stop_task = tokio::task::spawn_blocking(move || {
                    let pm = ProcessManager::new(ExecutionMode::Auto)?;
                    for runtime in &to_stop {
                        if let Ok(outcome) = pm.stop_with_timeout(runtime, timeout) {
                            if outcome.forced {
                                let _ = append_audit_file("runtime.force_kill", runtime, "ok");
                            }
                            let _ = append_audit_file("runtime.stop", runtime, "ok");
                        }
                    }
                    Ok::<(), anyhow::Error>(())
                });

                tokio::select! {
                    _ = tokio::signal::ctrl_c() => {
                        println!("Force stopping...");
                        for runtime in &started_runtimes {
                            if process_manager.force_kill(runtime)? {
                                append_audit_file("runtime.force_kill", runtime, "ok")?;
                            }
                        }
                    }
                    result = stop_task => {
                        result.map_err(|e| anyhow::anyhow!("shutdown task failed: {e}"))??;
                    }
                }
                break;
            }
            _ = tick.tick() => {
                for line in stream.drain() {
                    println!(
                        "{}",
                        render_log_line(&line.runtime, &line.text, !opts.no_log_prefix, None)
                    );
                }

                let all_stopped = match mode {
                    ExecutionMode::Docker => {
                        manager.list_agents().iter().all(|a| a.state != AgentState::Running)
                    }
                    _ => {
                        started_runtimes.iter().all(|runtime| !runtime_running(process_manager, runtime))
                    }
                };
                if all_stopped {
                    break;
                }
            }
        }
    }

    Ok(())
}

fn print_status_table(process_manager: &ProcessManager) -> Result<()> {
    let statuses = process_manager.list_statuses()?;
    if statuses.is_empty() {
        println!("No running runtimes");
        return Ok(());
    }

    println!(
        "{:<14} {:<8} {:<10} {:<10}",
        "RUNTIME", "PID", "STATE", "HEALTH"
    );
    for status in statuses {
        println!(
            "{:<14} {:<8} {:<10} {:<10}",
            status.runtime,
            status
                .pid
                .map(|pid| pid.to_string())
                .unwrap_or_else(|| "-".to_string()),
            if status.running { "running" } else { "stopped" },
            status.health,
        );
    }
    Ok(())
}

fn runtime_running(process_manager: &ProcessManager, runtime: &str) -> bool {
    process_manager
        .list_statuses()
        .ok()
        .and_then(|rows| {
            rows.into_iter()
                .find(|row| row.runtime == runtime)
                .map(|row| row.running)
        })
        .unwrap_or(false)
}

pub(crate) fn validate_direct_runtime_config(
    config: &ClawDenYaml,
    runtime: &str,
    env_vars: &[(String, String)],
    channels: &[String],
) -> Result<()> {
    let env_has = |key: &str| -> bool {
        env_vars
            .iter()
            .any(|(k, v)| k == key && !v.trim().is_empty())
    };

    // (scope, name, env_var, resolved, source)
    let mut fields: Vec<(&str, String, String, bool, &str)> = Vec::new();

    // --- Provider key check ---
    if let Some((provider_name, _, _)) = runtime_provider_and_model(config, runtime) {
        let resolved = env_has("CLAWDEN_LLM_API_KEY");
        fields.push((
            "provider",
            provider_name,
            "CLAWDEN_LLM_API_KEY".to_string(),
            resolved,
            if resolved { "provided" } else { "" },
        ));
    }

    // --- Channel credential checks (config struct AND env_vars) ---
    for channel_name in channels {
        let channel = config.channels.get(channel_name);
        let channel_type = channel
            .and_then(|ch| ClawDenYaml::resolve_channel_type(channel_name, ch))
            .unwrap_or_else(|| channel_name.clone());

        let config_has_token_or_bot = |ch: Option<&clawden_config::ChannelInstanceYaml>| -> bool {
            ch.and_then(|c| c.token.as_ref().or(c.bot_token.as_ref()))
                .is_some_and(|v| !v.trim().is_empty())
        };

        match channel_type.as_str() {
            "telegram" => {
                let cfg_ok = config_has_token_or_bot(channel);
                let env_ok = env_has("TELEGRAM_BOT_TOKEN");
                let resolved = cfg_ok || env_ok;
                let source = if cfg_ok {
                    "clawden.yaml"
                } else if env_ok {
                    "provided"
                } else {
                    ""
                };
                fields.push((
                    "channel",
                    channel_name.clone(),
                    "TELEGRAM_BOT_TOKEN".to_string(),
                    resolved,
                    source,
                ));
            }
            "discord" => {
                let cfg_ok = config_has_token_or_bot(channel);
                let env_ok = env_has("DISCORD_BOT_TOKEN");
                let resolved = cfg_ok || env_ok;
                let source = if cfg_ok {
                    "clawden.yaml"
                } else if env_ok {
                    "provided"
                } else {
                    ""
                };
                fields.push((
                    "channel",
                    channel_name.clone(),
                    "DISCORD_BOT_TOKEN".to_string(),
                    resolved,
                    source,
                ));
            }
            "slack" => {
                let cfg_bt = channel
                    .and_then(|c| c.bot_token.as_ref())
                    .is_some_and(|v| !v.trim().is_empty());
                let env_bt = env_has("SLACK_BOT_TOKEN");
                let r_bt = cfg_bt || env_bt;
                fields.push((
                    "channel",
                    channel_name.clone(),
                    "SLACK_BOT_TOKEN".to_string(),
                    r_bt,
                    if cfg_bt {
                        "clawden.yaml"
                    } else if env_bt {
                        "provided"
                    } else {
                        ""
                    },
                ));

                let cfg_at = channel
                    .and_then(|c| c.app_token.as_ref())
                    .is_some_and(|v| !v.trim().is_empty());
                let env_at = env_has("SLACK_APP_TOKEN");
                let r_at = cfg_at || env_at;
                fields.push((
                    "channel",
                    channel_name.clone(),
                    "SLACK_APP_TOKEN".to_string(),
                    r_at,
                    if cfg_at {
                        "clawden.yaml"
                    } else if env_at {
                        "provided"
                    } else {
                        ""
                    },
                ));
            }
            "signal" => {
                let cfg_p = channel
                    .and_then(|c| c.phone.as_ref())
                    .is_some_and(|v| !v.trim().is_empty());
                let env_p = env_has("SIGNAL_PHONE");
                let r_p = cfg_p || env_p;
                fields.push((
                    "channel",
                    channel_name.clone(),
                    "SIGNAL_PHONE".to_string(),
                    r_p,
                    if cfg_p {
                        "clawden.yaml"
                    } else if env_p {
                        "provided"
                    } else {
                        ""
                    },
                ));

                let cfg_t = channel
                    .and_then(|c| c.token.as_ref())
                    .is_some_and(|v| !v.trim().is_empty());
                let env_t = env_has("SIGNAL_TOKEN");
                let r_t = cfg_t || env_t;
                fields.push((
                    "channel",
                    channel_name.clone(),
                    "SIGNAL_TOKEN".to_string(),
                    r_t,
                    if cfg_t {
                        "clawden.yaml"
                    } else if env_t {
                        "provided"
                    } else {
                        ""
                    },
                ));
            }
            _ => {
                let env_var_name = format!(
                    "{}_BOT_TOKEN",
                    channel_type.to_ascii_uppercase().replace('-', "_")
                );
                let cfg_ok = config_has_token_or_bot(channel);
                let env_ok = env_has(&env_var_name);
                let resolved = cfg_ok || env_ok;
                let source = if cfg_ok {
                    "clawden.yaml"
                } else if env_ok {
                    "provided"
                } else {
                    ""
                };
                fields.push((
                    "channel",
                    channel_name.clone(),
                    env_var_name,
                    resolved,
                    source,
                ));
            }
        }
    }

    let has_missing = fields.iter().any(|(_, _, _, resolved, _)| !resolved);
    if !has_missing {
        return Ok(());
    }

    // --- Generate improved error message ---
    let mut lines = vec!["Required fields for this run:".to_string()];
    let mut last_scope = "";
    let mut last_name = String::new();
    for (scope, name, env_var, resolved, source) in &fields {
        if *scope != last_scope || *name != last_name {
            lines.push(format!("    {scope}: {name}"));
            last_scope = scope;
            last_name = name.clone();
        }
        let dots = ".".repeat(24usize.saturating_sub(env_var.len()));
        if *resolved {
            lines.push(format!("        - {env_var} {dots} \u{2713} ({source})"));
        } else {
            lines.push(format!("        - {env_var} {dots} \u{2717} missing"));
        }
    }

    // Host env hints for missing provider
    let provider_missing = fields.iter().any(|(s, _, _, r, _)| *s == "provider" && !r);
    let no_provider = !fields.iter().any(|(s, _, _, _, _)| *s == "provider");
    if provider_missing || no_provider {
        let candidates: &[(&str, &str)] = &[
            ("OPENROUTER_API_KEY", "openrouter"),
            ("OPENAI_API_KEY", "openai"),
            ("ANTHROPIC_API_KEY", "anthropic"),
            ("GEMINI_API_KEY", "google"),
            ("GOOGLE_API_KEY", "google"),
            ("MISTRAL_API_KEY", "mistral"),
            ("GROQ_API_KEY", "groq"),
        ];
        let mut found_hint = false;
        for (ev, prov) in candidates {
            if let Ok(val) = std::env::var(ev) {
                if !val.trim().is_empty() {
                    lines.push(format!(
                        "        \u{1F4A1} Detected {ev} in your environment \u{2014} add --provider {prov} to use it"
                    ));
                    found_hint = true;
                    break;
                }
            }
        }
        if !found_hint && no_provider {
            lines.push(
                "    \u{1F4A1} No provider configured. Try: --provider openrouter --api-key <key>"
                    .to_string(),
            );
            lines.push(
                "       Or set OPENROUTER_API_KEY in your environment / .env file".to_string(),
            );
        }
    }

    // Suggested command
    lines.push(String::new());
    let mut cmd_parts = vec!["clawden run".to_string()];
    let mut seen_token = false;
    let mut seen_app_token = false;
    let mut seen_phone = false;
    for (scope, name, env_var, resolved, _) in &fields {
        if *resolved {
            continue;
        }
        if *scope == "provider" {
            cmd_parts.push(format!("--provider {name}"));
            cmd_parts.push("--api-key <your-api-key>".to_string());
        } else {
            match env_var.as_str() {
                v if (v.ends_with("_BOT_TOKEN") || v == "SIGNAL_TOKEN") && !seen_token => {
                    cmd_parts.push("--token <your-token>".to_string());
                    seen_token = true;
                }
                v if v.ends_with("_APP_TOKEN") && !seen_app_token => {
                    cmd_parts.push("--app-token <your-app-token>".to_string());
                    seen_app_token = true;
                }
                v if v.ends_with("_PHONE") && !seen_phone => {
                    cmd_parts.push("--phone <your-phone>".to_string());
                    seen_phone = true;
                }
                _ => {
                    cmd_parts.push(format!("-e {env_var}=<value>"));
                }
            }
        }
    }
    for ch in channels {
        cmd_parts.push(format!("--channel {ch}"));
    }
    cmd_parts.push(runtime.to_string());

    lines.push("Suggested command:".to_string());
    lines.push(format!("    {}", cmd_parts.join(" ")));
    lines.push(String::new());
    lines.push("Or skip validation:".to_string());
    lines.push(format!(
        "    clawden run --allow-missing-credentials {}{}",
        if channels.is_empty() {
            String::new()
        } else {
            format!(
                "{} ",
                channels
                    .iter()
                    .map(|c| format!("--channel {c}"))
                    .collect::<Vec<_>>()
                    .join(" ")
            )
        },
        runtime
    ));

    anyhow::bail!(lines.join("\n"));
}

pub(crate) fn verify_runtime_startup(
    process_manager: &ProcessManager,
    runtime: &str,
    info: &ProcessInfo,
) -> Result<()> {
    std::thread::sleep(Duration::from_millis(500));
    if !runtime_running(process_manager, runtime) {
        let tail = process_manager.tail_logs(runtime, 50)?;
        anyhow::bail!("✗ {} exited immediately after startup\n{}", runtime, tail);
    }

    for _ in 0..3 {
        std::thread::sleep(Duration::from_millis(500));
        if !runtime_running(process_manager, runtime) {
            let tail = process_manager.tail_logs(runtime, 50)?;
            anyhow::bail!("✗ {} crashed on startup\n{}", runtime, tail);
        }
    }

    if let Some(url) = info.health_url.as_deref() {
        for _ in 0..5 {
            if health_probe_ok(url) {
                println!("✓ {} ready", runtime);
                return Ok(());
            }
            std::thread::sleep(Duration::from_secs(1));
            if !runtime_running(process_manager, runtime) {
                let tail = process_manager.tail_logs(runtime, 50)?;
                anyhow::bail!("✗ {} crashed on startup\n{}", runtime, tail);
            }
        }
        println!(
            "⚠ {} started (pid {}) but health check not responding",
            runtime, info.pid
        );
        return Ok(());
    }

    println!("✓ {} ready", runtime);
    Ok(())
}

fn health_probe_ok(url: &str) -> bool {
    std::process::Command::new("curl")
        .args(["-fsS", "--max-time", "2", url])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

pub(crate) fn render_log_line(
    runtime: &str,
    text: &str,
    use_prefix: bool,
    timestamp_ms: Option<u64>,
) -> String {
    let mut body = String::new();
    if let Some(ts) = timestamp_ms {
        body.push_str(&format!("[{}] ", ts / 1000));
    }

    if use_prefix {
        body.push_str(color_prefix(runtime).as_str());
        body.push_str(text);
        body
    } else {
        body.push_str(text);
        body
    }
}

fn color_prefix(runtime: &str) -> String {
    const COLORS: [&str; 6] = ["36", "33", "32", "35", "34", "31"];
    let mut hash = 0usize;
    for b in runtime.as_bytes() {
        hash = hash.wrapping_add(*b as usize);
    }
    let color = COLORS[hash % COLORS.len()];
    format!("\x1b[{color}m{:<10} |\x1b[0m ", runtime)
}

pub(crate) fn load_config() -> Result<Option<ClawDenYaml>> {
    load_config_with_env_file(None)
}

pub(crate) fn load_config_with_env_file(env_file: Option<&str>) -> Result<Option<ClawDenYaml>> {
    let yaml_path = std::env::current_dir()?.join("clawden.yaml");
    if !yaml_path.exists() {
        return Ok(None);
    }
    if let Some(path) = env_file {
        debug!("loading env file override: {}", path);
        dotenvy::from_path_override(path)
            .map_err(|e| anyhow::anyhow!("failed to load {path}: {e}"))?;
    }

    let mut cfg = ClawDenYaml::from_file(&yaml_path).map_err(|e| anyhow::anyhow!("{}", e))?;
    if let Err(errs) = cfg.resolve_env_vars() {
        anyhow::bail!(
            "failed to resolve environment variables in clawden.yaml:\n{}",
            errs.join("\n")
        );
    }
    if let Err(errs) = cfg.validate() {
        anyhow::bail!("clawden.yaml validation failed:\n{}", errs.join("\n"));
    }
    Ok(Some(cfg))
}

pub(crate) fn parse_env_overrides(entries: &[String]) -> Result<Vec<(String, String)>> {
    let mut env = Vec::new();
    for raw in entries {
        if let Some((key, value)) = raw.split_once('=') {
            if key.trim().is_empty() {
                anyhow::bail!("invalid --env entry '{raw}': missing key");
            }
            env.push((key.trim().to_string(), value.to_string()));
            continue;
        }

        let key = raw.trim();
        if key.is_empty() {
            anyhow::bail!("invalid --env entry '{raw}': missing key");
        }
        let value = std::env::var(key).unwrap_or_default();
        env.push((key.to_string(), value));
    }
    Ok(env)
}

pub fn resolve_target_runtimes(
    runtimes: Vec<String>,
    config: Option<&ClawDenYaml>,
    installer: &RuntimeInstaller,
) -> Result<Vec<String>> {
    let mut resolved = if !runtimes.is_empty() {
        runtimes
    } else if let Some(cfg) = config {
        runtimes_from_config(cfg)
    } else {
        installer
            .list_installed()?
            .into_iter()
            .map(|row| row.runtime)
            .collect::<Vec<_>>()
    };

    resolved.sort();
    resolved.dedup();
    Ok(resolved)
}

/// Extract runtime names from a parsed clawden.yaml config.
pub fn runtimes_from_config(config: &ClawDenYaml) -> Vec<String> {
    if let Some(rt) = &config.runtime {
        vec![rt.clone()]
    } else {
        config.runtimes.iter().map(|r| r.name.clone()).collect()
    }
}

/// Extract channel names relevant to a specific runtime from clawden.yaml.
pub(crate) fn channels_for_runtime(config: &ClawDenYaml, runtime: &str) -> Vec<String> {
    // Single-runtime shorthand: all channels belong to this runtime
    if config.runtime.as_deref() == Some(runtime) {
        return config.channels.keys().cloned().collect();
    }
    // Multi-runtime: use the channel list from the runtime entry
    if let Some(entry) = config.runtimes.iter().find(|e| e.name == runtime) {
        return entry.channels.clone();
    }
    Vec::new()
}

pub(crate) fn pinned_version_for_runtime<'a>(
    config: &'a ClawDenYaml,
    runtime: &str,
) -> Option<&'a str> {
    if config.runtime.as_deref() == Some(runtime) {
        return config.version.as_deref();
    }
    config
        .runtimes
        .iter()
        .find(|entry| entry.name == runtime)
        .and_then(|entry| entry.version.as_deref())
}

pub(crate) fn tools_for_runtime(config: &ClawDenYaml, runtime: &str) -> Vec<String> {
    if config.runtime.as_deref() == Some(runtime) {
        return config.tools.clone();
    }
    config
        .runtimes
        .iter()
        .find(|entry| entry.name == runtime)
        .map(|entry| entry.tools.clone())
        .unwrap_or_default()
}

/// Build all env vars a runtime needs: LLM provider config + channel credentials.
/// Public so `exec_run` can reuse this.
pub fn build_runtime_env_vars(
    config: &ClawDenYaml,
    runtime: &str,
) -> Result<Vec<(String, String)>> {
    let mut env = HashMap::new();

    // --- LLM provider env vars ---
    if let Some((provider_name, mut provider, model)) = runtime_provider_and_model(config, runtime)
    {
        if provider.api_key.is_none() {
            provider.api_key = get_provider_key_from_vault(&provider_name)?;
        }

        let provider_type = provider
            .provider_type
            .clone()
            .or_else(|| infer_provider_type(&provider_name));
        let provider_label = provider_type
            .as_ref()
            .map(provider_slug)
            .unwrap_or_else(|| provider_name.to_ascii_lowercase());
        let runtime_key = runtime.to_ascii_uppercase().replace('-', "_");

        env.insert("CLAWDEN_LLM_PROVIDER".to_string(), provider_label.clone());
        env.insert(format!("{runtime_key}_LLM_PROVIDER"), provider_label);

        if let Some(model_name) = model {
            env.insert("CLAWDEN_LLM_MODEL".to_string(), model_name.clone());
            env.insert(format!("{runtime_key}_LLM_MODEL"), model_name);
        }

        if let Some(api_key) = provider.api_key {
            env.insert("CLAWDEN_LLM_API_KEY".to_string(), api_key.clone());
            env.insert(format!("{runtime_key}_LLM_API_KEY"), api_key.clone());
            for env_name in provider_key_env_names(provider_type.as_ref(), &provider_name) {
                env.insert(env_name.to_string(), api_key.clone());
            }
        }

        if let Some(base_url) = provider.base_url {
            env.insert("CLAWDEN_LLM_BASE_URL".to_string(), base_url.clone());
            env.insert(format!("{runtime_key}_LLM_BASE_URL"), base_url);
        }

        if let Some(org_id) = provider.org_id {
            env.insert("CLAWDEN_LLM_ORG_ID".to_string(), org_id.clone());
            env.insert(format!("{runtime_key}_LLM_ORG_ID"), org_id);
        }
    }

    // --- Channel credential env vars ---
    let channel_names = channels_for_runtime(config, runtime);
    let runtime_slug = runtime.to_ascii_lowercase().replace('-', "");
    for ch_name in &channel_names {
        if let Some(ch_instance) = config.channels.get(ch_name) {
            let ch_type = ClawDenYaml::resolve_channel_type(ch_name, ch_instance)
                .unwrap_or_else(|| ch_name.clone());

            // Pass through canonical channel token env vars so runtimes that
            // read the standard names (e.g. TELEGRAM_BOT_TOKEN) also work,
            // especially in Docker mode where host env is not inherited.
            if let Some(token) = &ch_instance.token {
                let canonical = format!("{}_BOT_TOKEN", ch_type.to_uppercase());
                env.entry(canonical).or_insert_with(|| token.clone());
            }
            if let Some(bt) = &ch_instance.bot_token {
                let canonical = format!("{}_BOT_TOKEN", ch_type.to_uppercase());
                env.entry(canonical).or_insert_with(|| bt.clone());
            }
            if let Some(at) = &ch_instance.app_token {
                let canonical = format!("{}_APP_TOKEN", ch_type.to_uppercase());
                env.entry(canonical).or_insert_with(|| at.clone());
            }

            // Use runtime-specific env var mappers where available
            let channel_vars = match runtime_slug.as_str() {
                "zeroclaw" => ChannelCredentialMapper::zeroclaw_env_vars(&ch_type, ch_instance),
                "nanoclaw" => ChannelCredentialMapper::nanoclaw_env_vars(&ch_type, ch_instance),
                "openclaw" => ChannelCredentialMapper::openclaw_env_vars(&ch_type, ch_instance),
                _ => ChannelCredentialMapper::zeroclaw_env_vars(&ch_type, ch_instance),
            };
            env.extend(channel_vars);
        }
    }

    let mut pairs: Vec<_> = env.into_iter().collect();
    pairs.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(pairs)
}

pub(crate) fn runtime_provider_and_model(
    config: &ClawDenYaml,
    runtime: &str,
) -> Option<(String, ProviderEntryYaml, Option<String>)> {
    if let Some(single_runtime) = &config.runtime {
        if single_runtime == runtime {
            let provider_name = match config.provider.as_ref() {
                Some(ProviderRefYaml::Name(name)) => name.clone(),
                Some(ProviderRefYaml::Inline(entry)) => config
                    .providers
                    .keys()
                    .next()
                    .cloned()
                    .or_else(|| entry.provider_type.as_ref().map(provider_slug))
                    .unwrap_or_else(|| "provider".to_string()),
                None => return None,
            };

            let provider =
                match config.provider.as_ref() {
                    Some(ProviderRefYaml::Inline(entry)) => entry.clone(),
                    Some(ProviderRefYaml::Name(name)) => config
                        .providers
                        .get(name)
                        .cloned()
                        .unwrap_or(ProviderEntryYaml {
                            provider_type: infer_provider_type(name),
                            api_key: None,
                            base_url: None,
                            org_id: None,
                            extra: HashMap::new(),
                        }),
                    None => return None,
                };
            return Some((provider_name, provider, config.model.clone()));
        }
    }

    let entry = config.runtimes.iter().find(|entry| entry.name == runtime)?;
    let provider_name = entry.provider.clone()?;
    let provider = config
        .providers
        .get(&provider_name)
        .cloned()
        .unwrap_or(ProviderEntryYaml {
            provider_type: infer_provider_type(&provider_name),
            api_key: None,
            base_url: None,
            org_id: None,
            extra: HashMap::new(),
        });

    Some((provider_name, provider, entry.model.clone()))
}

pub(crate) fn infer_provider_type(name: &str) -> Option<LlmProvider> {
    match name.to_ascii_lowercase().as_str() {
        "openai" => Some(LlmProvider::OpenAi),
        "anthropic" => Some(LlmProvider::Anthropic),
        "google" => Some(LlmProvider::Google),
        "mistral" => Some(LlmProvider::Mistral),
        "groq" => Some(LlmProvider::Groq),
        "openrouter" => Some(LlmProvider::OpenRouter),
        "ollama" => Some(LlmProvider::Ollama),
        _ => None,
    }
}

fn provider_slug(provider: &LlmProvider) -> String {
    match provider {
        LlmProvider::OpenAi => "openai".to_string(),
        LlmProvider::Anthropic => "anthropic".to_string(),
        LlmProvider::Google => "google".to_string(),
        LlmProvider::Mistral => "mistral".to_string(),
        LlmProvider::Groq => "groq".to_string(),
        LlmProvider::OpenRouter => "openrouter".to_string(),
        LlmProvider::Ollama => "ollama".to_string(),
        LlmProvider::Custom(name) => name.to_ascii_lowercase(),
    }
}

fn provider_key_env_names(
    provider_type: Option<&LlmProvider>,
    provider_name: &str,
) -> Vec<&'static str> {
    let resolved = provider_type
        .cloned()
        .or_else(|| infer_provider_type(provider_name));
    match resolved.as_ref() {
        Some(LlmProvider::OpenAi) => vec!["OPENAI_API_KEY"],
        Some(LlmProvider::Anthropic) => vec!["ANTHROPIC_API_KEY"],
        Some(LlmProvider::Google) => vec!["GOOGLE_API_KEY", "GEMINI_API_KEY"],
        Some(LlmProvider::Mistral) => vec!["MISTRAL_API_KEY"],
        Some(LlmProvider::Groq) => vec!["GROQ_API_KEY"],
        Some(LlmProvider::OpenRouter) => vec!["OPENROUTER_API_KEY"],
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        build_runtime_env_vars, channels_for_runtime, parse_env_overrides,
        validate_direct_runtime_config, verify_runtime_startup, ClawDenYaml,
    };
    use crate::commands::test_env_lock;
    use clawden_core::{ExecutionMode, ProcessManager};
    use std::fs;
    use std::net::TcpListener;
    use std::os::unix::fs::PermissionsExt;
    use std::path::Path;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn runtime_env_vars_include_provider_key_and_model() {
        let yaml = r#"
runtimes:
  - name: zeroclaw
    provider: openai
    model: gpt-4o-mini
providers:
  openai:
    api_key: sk-test
"#;
        let mut config = ClawDenYaml::parse_yaml(yaml).expect("yaml should parse");
        config
            .resolve_env_vars()
            .expect("env vars should resolve without references");

        let env = build_runtime_env_vars(&config, "zeroclaw").expect("env vars should build");
        assert!(env
            .iter()
            .any(|(k, v)| k == "OPENAI_API_KEY" && v == "sk-test"));
        assert!(env
            .iter()
            .any(|(k, v)| k == "CLAWDEN_LLM_MODEL" && v == "gpt-4o-mini"));
        assert!(env
            .iter()
            .any(|(k, v)| k == "ZEROCLAW_LLM_API_KEY" && v == "sk-test"));
    }

    #[test]
    fn runtime_env_vars_support_env_only_runtime_openclaw() {
        let yaml = r#"
runtime: openclaw
provider: openai
model: gpt-4o-mini
providers:
  openai:
    api_key: sk-test
channels:
  bot:
    type: telegram
    token: tg-test
    allowed_users: ["12345", "67890"]
"#;
        let mut config = ClawDenYaml::parse_yaml(yaml).expect("yaml should parse");
        config
            .resolve_env_vars()
            .expect("env vars should resolve without references");

        let env = build_runtime_env_vars(&config, "openclaw").expect("env vars should build");
        assert!(env
            .iter()
            .any(|(k, v)| k == "OPENAI_API_KEY" && v == "sk-test"));
        assert!(env
            .iter()
            .any(|(k, v)| k == "TELEGRAM_BOT_TOKEN" && v == "tg-test"));
        assert!(env
            .iter()
            .any(|(k, v)| k == "OPENCLAW_LLM_PROVIDER" && v == "openai"));
        assert!(env
            .iter()
            .any(|(k, v)| k == "OPENCLAW_TELEGRAM_ALLOW_FROM" && v == "12345,67890"));
    }

    #[test]
    fn direct_validation_rejects_empty_channel_token() {
        let yaml = r#"
runtime: zeroclaw
provider: openrouter
providers:
  openrouter:
    api_key: sk-test
channels:
  support-tg:
    type: telegram
    token: ""
"#;
        let mut config = ClawDenYaml::parse_yaml(yaml).expect("yaml should parse");
        config.resolve_env_vars().expect("env vars should resolve");
        let channels = channels_for_runtime(&config, "zeroclaw");
        let env = build_runtime_env_vars(&config, "zeroclaw").expect("env vars should build");
        let err = validate_direct_runtime_config(&config, "zeroclaw", &env, &channels)
            .expect_err("empty telegram token must fail");
        assert!(err.to_string().contains("Required fields for this run"));
        assert!(err.to_string().contains("TELEGRAM_BOT_TOKEN"));
    }

    #[test]
    fn direct_validation_rejects_missing_provider_api_key() {
        let yaml = r#"
runtime: zeroclaw
provider: openrouter
providers:
  openrouter:
    base_url: https://openrouter.ai/api/v1
channels:
  support-tg:
    type: telegram
    token: tg-token
"#;
        let mut config = ClawDenYaml::parse_yaml(yaml).expect("yaml should parse");
        config.resolve_env_vars().expect("env vars should resolve");
        let channels = channels_for_runtime(&config, "zeroclaw");
        let err = validate_direct_runtime_config(&config, "zeroclaw", &[], &channels)
            .expect_err("missing provider key must fail");
        assert!(err.to_string().contains("provider: openrouter"));
        assert!(err.to_string().contains("CLAWDEN_LLM_API_KEY"));
        assert!(err.to_string().contains("--allow-missing-credentials"));
    }

    #[test]
    fn startup_check_detects_immediate_crash() {
        let _guard = test_env_lock().lock().expect("env lock poisoned");
        let original_home = std::env::var("HOME").ok();
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        let tmp_home = std::env::temp_dir().join(format!("clawden-up-test-{unique}"));
        fs::create_dir_all(&tmp_home).expect("tmp home");
        std::env::set_var("HOME", &tmp_home);

        let manager = ProcessManager::new(ExecutionMode::Direct).expect("process manager");
        let script = tmp_home.join("crash-runtime.sh");
        write_executable(&script, "#!/usr/bin/env sh\necho boom\nexit 1\n");
        let info = manager
            .start_direct_with_env_and_project("testruntime", &script, &[], &[], Some("ph".into()))
            .expect("runtime should start");
        let err = verify_runtime_startup(&manager, "testruntime", &info)
            .expect_err("startup checker should fail");
        let msg = err.to_string();
        assert!(msg.contains("crashed on startup") || msg.contains("exited immediately"));

        if let Some(home) = original_home {
            std::env::set_var("HOME", home);
        } else {
            std::env::remove_var("HOME");
        }
        let _ = fs::remove_dir_all(tmp_home);
    }

    #[test]
    fn startup_check_warns_when_health_not_responding_but_process_alive() {
        let _guard = test_env_lock().lock().expect("env lock poisoned");
        let original_home = std::env::var("HOME").ok();
        let original_health = std::env::var("CLAWDEN_HEALTH_URL_ZEROCLAW").ok();
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        let tmp_home = std::env::temp_dir().join(format!("clawden-up-test-warn-{unique}"));
        fs::create_dir_all(&tmp_home).expect("tmp home");
        std::env::set_var("HOME", &tmp_home);
        std::env::set_var("CLAWDEN_HEALTH_URL_ZEROCLAW", "http://127.0.0.1:9/health");

        let manager = ProcessManager::new(ExecutionMode::Direct).expect("process manager");
        let script = tmp_home.join("slow-runtime.sh");
        write_executable(&script, "#!/usr/bin/env sh\nsleep 8\n");
        let info = manager
            .start_direct_with_env_and_project("zeroclaw", &script, &[], &[], Some("ph".into()))
            .expect("runtime should start");
        verify_runtime_startup(&manager, "zeroclaw", &info)
            .expect("startup check should warn and continue");

        let _ = manager.stop_with_timeout("zeroclaw", 1);
        if let Some(url) = original_health {
            std::env::set_var("CLAWDEN_HEALTH_URL_ZEROCLAW", url);
        } else {
            std::env::remove_var("CLAWDEN_HEALTH_URL_ZEROCLAW");
        }
        if let Some(home) = original_home {
            std::env::set_var("HOME", home);
        } else {
            std::env::remove_var("HOME");
        }
        let _ = fs::remove_dir_all(tmp_home);
    }

    #[test]
    fn startup_check_handles_openfang_and_zeroclaw_independently() {
        let _guard = test_env_lock().lock().expect("env lock poisoned");
        let original_home = std::env::var("HOME").ok();
        let original_zero_url = std::env::var("CLAWDEN_HEALTH_URL_ZEROCLAW").ok();
        let original_openfang_url = std::env::var("CLAWDEN_HEALTH_URL_OPENFANG").ok();
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        let tmp_home = std::env::temp_dir().join(format!("clawden-up-test-multi-{unique}"));
        fs::create_dir_all(&tmp_home).expect("tmp home");
        std::env::set_var("HOME", &tmp_home);

        let zeroclaw_port = reserve_local_port();
        let openfang_port = reserve_local_port();

        std::env::set_var(
            "CLAWDEN_HEALTH_URL_ZEROCLAW",
            format!("http://127.0.0.1:{zeroclaw_port}/health"),
        );
        std::env::set_var(
            "CLAWDEN_HEALTH_URL_OPENFANG",
            format!("http://127.0.0.1:{openfang_port}/health"),
        );

        let manager = ProcessManager::new(ExecutionMode::Direct).expect("process manager");

        let zeroclaw_script = tmp_home.join("zeroclaw-health.py");
        let zeroclaw_body = format!(
            "#!/usr/bin/env python3\nimport http.server\nimport socketserver\n\nclass H(http.server.BaseHTTPRequestHandler):\n    def do_GET(self):\n        if self.path == '/health':\n            self.send_response(200)\n            self.end_headers()\n            self.wfile.write(b'ok')\n        else:\n            self.send_response(404)\n            self.end_headers()\n    def log_message(self, format, *args):\n        return\n\nwith socketserver.TCPServer(('127.0.0.1', {zeroclaw_port}), H) as s:\n    s.serve_forever()\n"
        );
        write_executable(&zeroclaw_script, &zeroclaw_body);

        let openfang_script = tmp_home.join("openfang-health.py");
        let openfang_body = format!(
            "#!/usr/bin/env python3\nimport http.server\nimport socketserver\n\nclass H(http.server.BaseHTTPRequestHandler):\n    def do_GET(self):\n        if self.path == '/health':\n            self.send_response(200)\n            self.end_headers()\n            self.wfile.write(b'ok')\n        else:\n            self.send_response(404)\n            self.end_headers()\n    def log_message(self, format, *args):\n        return\n\nwith socketserver.TCPServer(('127.0.0.1', {openfang_port}), H) as s:\n    s.serve_forever()\n"
        );
        write_executable(&openfang_script, &openfang_body);

        let zero_info = manager
            .start_direct_with_env_and_project(
                "zeroclaw",
                &zeroclaw_script,
                &[],
                &[],
                Some("multi-ph".into()),
            )
            .expect("zeroclaw should start");
        verify_runtime_startup(&manager, "zeroclaw", &zero_info)
            .expect("zeroclaw health check should pass");

        let openfang_info = manager
            .start_direct_with_env_and_project(
                "openfang",
                &openfang_script,
                &[],
                &[],
                Some("multi-ph".into()),
            )
            .expect("openfang should start");
        verify_runtime_startup(&manager, "openfang", &openfang_info)
            .expect("openfang health check should pass");

        let _ = manager.stop_with_timeout("zeroclaw", 1);
        let _ = manager.stop_with_timeout("openfang", 1);

        if let Some(url) = original_zero_url {
            std::env::set_var("CLAWDEN_HEALTH_URL_ZEROCLAW", url);
        } else {
            std::env::remove_var("CLAWDEN_HEALTH_URL_ZEROCLAW");
        }
        if let Some(url) = original_openfang_url {
            std::env::set_var("CLAWDEN_HEALTH_URL_OPENFANG", url);
        } else {
            std::env::remove_var("CLAWDEN_HEALTH_URL_OPENFANG");
        }
        if let Some(home) = original_home {
            std::env::set_var("HOME", home);
        } else {
            std::env::remove_var("HOME");
        }
        let _ = fs::remove_dir_all(tmp_home);
    }

    fn write_executable(path: &Path, body: &str) {
        fs::write(path, body).expect("script should be written");
        let mut perms = fs::metadata(path).expect("metadata").permissions();
        perms.set_mode(0o755);
        fs::set_permissions(path, perms).expect("chmod");
    }

    fn reserve_local_port() -> u16 {
        TcpListener::bind("127.0.0.1:0")
            .expect("bind ephemeral port")
            .local_addr()
            .expect("local addr")
            .port()
    }

    #[test]
    fn parse_env_overrides_supports_key_value_and_key_only() {
        let _guard = test_env_lock().lock().expect("env lock");
        std::env::set_var("CLAWDEN_TEST_ENV_ONLY", "from-host");
        let vars = parse_env_overrides(&[
            "A=1".to_string(),
            "CLAWDEN_TEST_ENV_ONLY".to_string(),
            "A=2".to_string(),
        ])
        .expect("env vars should parse");
        assert_eq!(vars[0], ("A".to_string(), "1".to_string()));
        assert_eq!(
            vars[1],
            ("CLAWDEN_TEST_ENV_ONLY".to_string(), "from-host".to_string())
        );
        assert_eq!(vars[2], ("A".to_string(), "2".to_string()));
        std::env::remove_var("CLAWDEN_TEST_ENV_ONLY");
    }

    #[test]
    fn parse_env_overrides_rejects_empty_key() {
        let err = parse_env_overrides(&["=value".to_string()]).expect_err("empty key should fail");
        assert!(err.to_string().contains("missing key"));
    }
}
