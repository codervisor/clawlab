use anyhow::Result;
use clawden_config::{ChannelInstanceYaml, ClawDenYaml, ProviderEntryYaml, ProviderRefYaml};
use clawden_core::{
    validate_runtime_args, ExecutionMode, LifecycleManager, ProcessManager, RuntimeInstaller,
};
use std::collections::HashMap;
use std::fs;
use std::time::Duration;
use tracing::{debug, warn};

use crate::commands::config_gen::{generate_config_dir, inject_config_dir_arg};
use crate::commands::up::{
    build_runtime_env_vars, channels_for_runtime, infer_provider_type, load_config_with_env_file,
    parse_env_overrides, pinned_version_for_runtime, render_log_line, tools_for_runtime,
    validate_direct_runtime_config, verify_runtime_startup,
};
use crate::util::{
    append_audit_file, ensure_installed_runtime, env_no_docker_enabled, parse_runtime, project_hash,
};

pub struct RunOptions {
    pub runtime: String,
    pub channel: Vec<String>,
    pub env_vars: Vec<String>,
    pub env_file: Option<String>,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub token: Option<String>,
    pub api_key: Option<String>,
    pub app_token: Option<String>,
    pub phone: Option<String>,
    pub system_prompt: Option<String>,
    pub ports: Vec<String>,
    pub allow_missing_credentials: bool,
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

    let mut config = load_config_with_env_file(opts.env_file.as_deref())?;
    if let Some(cfg) = config.as_mut() {
        apply_run_overrides(cfg, &opts)?;
    }

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
    let mut env_vars = if let Some(cfg) = config.as_ref() {
        build_runtime_env_vars(cfg, &opts.runtime)?
    } else {
        Vec::new()
    };
    apply_shortcut_env_overrides(&mut env_vars, &opts)?;
    let env_overrides = parse_env_overrides(&opts.env_vars)?;
    if !env_overrides.is_empty() {
        let keys = env_overrides
            .iter()
            .map(|(k, _)| k.as_str())
            .collect::<Vec<_>>()
            .join(",");
        let _ = append_audit_file("runtime.env_override", &opts.runtime, &keys);
    }

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
            let mut docker_env = env_vars.clone();
            if !opts.ports.is_empty() {
                docker_env.push(("CLAWDEN_PORT_MAP".to_string(), opts.ports.join(",")));
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
                    env_vars: merge_env_overrides(docker_env, &env_overrides),
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
        if !opts.allow_missing_credentials {
            validate_direct_runtime_config(cfg, &opts.runtime, &combined_env, &resolved_channels)?;
        } else {
            warn!("missing credential checks are skipped (--allow-missing-credentials)");
        }
    }
    if !resolved_channels.is_empty() {
        combined_env.push(("CLAWDEN_CHANNELS".to_string(), resolved_channels.join(",")));
    }
    if !effective_tools.is_empty() {
        combined_env.push(("CLAWDEN_TOOLS".to_string(), effective_tools.join(",")));
    }
    if !opts.ports.is_empty() {
        combined_env.push(("CLAWDEN_PORT_MAP".to_string(), opts.ports.join(",")));
    }
    combined_env = merge_env_overrides(combined_env, &env_overrides);

    let info = process_manager.start_direct_with_env_and_project(
        &opts.runtime,
        &installed.executable,
        &args,
        &combined_env,
        Some(current_project_hash),
    )?;
    // Start capturing logs immediately after launch (while the log file is
    // still near-empty) so that startup output is not lost.  Without this,
    // stream_logs would begin from the current file size — after
    // verify_runtime_startup has already consumed ~2 s of output.
    let stream = process_manager.stream_logs(std::slice::from_ref(&opts.runtime))?;
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

fn merge_env_overrides(
    mut env_vars: Vec<(String, String)>,
    overrides: &[(String, String)],
) -> Vec<(String, String)> {
    for (key, value) in overrides {
        env_vars.retain(|(k, _)| k != key);
        env_vars.push((key.clone(), value.clone()));
    }
    env_vars
}

fn apply_shortcut_env_overrides(
    env_vars: &mut Vec<(String, String)>,
    opts: &RunOptions,
) -> Result<()> {
    let mut set_env = |key: String, value: String| {
        env_vars.retain(|(k, _)| *k != key);
        env_vars.push((key, value));
    };
    let runtime_key = opts.runtime.to_ascii_uppercase().replace('-', "_");
    if let Some(model) = &opts.model {
        set_env("CLAWDEN_LLM_MODEL".to_string(), model.clone());
        set_env(format!("{runtime_key}_LLM_MODEL"), model.clone());
    }
    if let Some(provider) = &opts.provider {
        set_env("CLAWDEN_LLM_PROVIDER".to_string(), provider.clone());
        set_env(format!("{runtime_key}_LLM_PROVIDER"), provider.clone());
    }
    if let Some(api_key) = &opts.api_key {
        set_env("CLAWDEN_LLM_API_KEY".to_string(), api_key.clone());
        set_env("CLAWDEN_API_KEY".to_string(), api_key.clone());
        set_env(format!("{runtime_key}_LLM_API_KEY"), api_key.clone());
        if let Some(provider) = opts.provider.as_ref() {
            if let Some(known) = infer_provider_type(provider) {
                for key in provider_env_key_aliases(&known) {
                    set_env(key.to_string(), api_key.clone());
                }
            }
        } else {
            eprintln!(
                "Hint: provider-specific API key env vars were skipped; pass --provider to map --api-key to OPENAI_API_KEY/ANTHROPIC_API_KEY."
            );
        }
    }
    if let Some(system_prompt) = &opts.system_prompt {
        set_env(
            "CLAWDEN_SYSTEM_PROMPT".to_string(),
            read_system_prompt(system_prompt)?,
        );
    }
    if let Some(token) = &opts.token {
        if opts.channel.is_empty() {
            anyhow::bail!(
                "Required fields for this run:\n    channel: (not selected)\n        - CHANNEL .............. missing\n\nHow to continue:\n    1) Select a channel explicitly: --channel telegram (or discord/slack/signal)\n    2) Then pass token(s): --token ..., --app-token ..., --phone ... as needed"
            );
        }
        for channel in &opts.channel {
            set_env(channel_token_env_name(channel), token.clone());
        }
    }
    if let Some(app_token) = &opts.app_token {
        for channel in &opts.channel {
            set_env(
                format!(
                    "{}_APP_TOKEN",
                    channel.to_ascii_uppercase().replace('-', "_")
                ),
                app_token.clone(),
            );
        }
    }
    if let Some(phone) = &opts.phone {
        for channel in &opts.channel {
            set_env(
                format!("{}_PHONE", channel.to_ascii_uppercase().replace('-', "_")),
                phone.clone(),
            );
        }
    }
    Ok(())
}

fn apply_run_overrides(config: &mut ClawDenYaml, opts: &RunOptions) -> Result<()> {
    if let Some(provider) = &opts.provider {
        if config.runtime.as_deref() == Some(&opts.runtime) {
            config.provider = Some(ProviderRefYaml::Name(provider.clone()));
        } else if let Some(entry) = config.runtimes.iter_mut().find(|r| r.name == opts.runtime) {
            entry.provider = Some(provider.clone());
        }
    }
    if let Some(model) = &opts.model {
        if config.runtime.as_deref() == Some(&opts.runtime) {
            config.model = Some(model.clone());
        } else if let Some(entry) = config.runtimes.iter_mut().find(|r| r.name == opts.runtime) {
            entry.model = Some(model.clone());
        }
    }
    if let Some(api_key) = &opts.api_key {
        let provider_name = opts.provider.clone().or_else(|| {
            super::up::runtime_provider_and_model(config, &opts.runtime).map(|(name, _, _)| name)
        });
        if let Some(provider_name) = provider_name {
            let entry =
                config
                    .providers
                    .entry(provider_name.clone())
                    .or_insert(ProviderEntryYaml {
                        provider_type: infer_provider_type(&provider_name),
                        api_key: None,
                        base_url: None,
                        org_id: None,
                        extra: HashMap::new(),
                    });
            entry.api_key = Some(api_key.clone());
        }
    }
    if let Some(system_prompt) = &opts.system_prompt {
        let val = serde_json::Value::String(read_system_prompt(system_prompt)?);
        if config.runtime.as_deref() == Some(&opts.runtime) {
            config.config.insert("system_prompt".to_string(), val);
        } else if let Some(entry) = config.runtimes.iter_mut().find(|r| r.name == opts.runtime) {
            entry.config.insert("system_prompt".to_string(), val);
        }
    }
    if opts.token.is_some() || opts.app_token.is_some() || opts.phone.is_some() {
        if opts.channel.is_empty() {
            anyhow::bail!("--token/--app-token/--phone require explicit --channel values");
        }
        for channel_name in &opts.channel {
            let channel = config
                .channels
                .entry(channel_name.clone())
                .or_insert_with(empty_channel_instance);
            if let Some(token) = &opts.token {
                channel.token = Some(token.clone());
            }
            if let Some(app_token) = &opts.app_token {
                channel.app_token = Some(app_token.clone());
            }
            if let Some(phone) = &opts.phone {
                channel.phone = Some(phone.clone());
            }
        }
    }
    debug!("applied run option overrides for runtime {}", opts.runtime);
    Ok(())
}

fn empty_channel_instance() -> ChannelInstanceYaml {
    ChannelInstanceYaml {
        channel_type: None,
        token: None,
        bot_token: None,
        app_token: None,
        phone: None,
        guild: None,
        allowed_users: Vec::new(),
        allowed_roles: Vec::new(),
        allowed_channels: Vec::new(),
        group_mode: None,
        extra: HashMap::new(),
    }
}

fn read_system_prompt(value: &str) -> Result<String> {
    if let Some(path) = value.strip_prefix('@') {
        return Ok(fs::read_to_string(path)?);
    }
    Ok(value.to_string())
}

fn channel_token_env_name(channel: &str) -> String {
    format!(
        "{}_BOT_TOKEN",
        channel.to_ascii_uppercase().replace('-', "_")
    )
}

fn provider_env_key_aliases(provider: &clawden_config::LlmProvider) -> &'static [&'static str] {
    match provider {
        clawden_config::LlmProvider::OpenAi => &["OPENAI_API_KEY"],
        clawden_config::LlmProvider::Anthropic => &["ANTHROPIC_API_KEY"],
        clawden_config::LlmProvider::Google => &["GOOGLE_API_KEY", "GEMINI_API_KEY"],
        clawden_config::LlmProvider::Mistral => &["MISTRAL_API_KEY"],
        clawden_config::LlmProvider::Groq => &["GROQ_API_KEY"],
        clawden_config::LlmProvider::OpenRouter => &["OPENROUTER_API_KEY"],
        _ => &[],
    }
}

#[cfg(test)]
mod tests {
    use super::{
        apply_run_overrides, apply_shortcut_env_overrides, merge_env_overrides, RunOptions,
    };
    use crate::commands::up::build_runtime_env_vars;
    use clawden_config::ClawDenYaml;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn make_opts() -> RunOptions {
        RunOptions {
            runtime: "zeroclaw".to_string(),
            channel: Vec::new(),
            env_vars: Vec::new(),
            env_file: None,
            provider: None,
            model: None,
            token: None,
            api_key: None,
            app_token: None,
            phone: None,
            system_prompt: None,
            ports: Vec::new(),
            allow_missing_credentials: false,
            tools: None,
            restart: None,
            detach: false,
            rm: false,
            extra_args: Vec::new(),
            no_docker: true,
        }
    }

    #[test]
    fn api_key_shortcut_sets_compat_aliases() {
        let mut opts = make_opts();
        opts.provider = Some("openai".to_string());
        opts.api_key = Some("sk-test".to_string());
        let mut env = Vec::new();
        apply_shortcut_env_overrides(&mut env, &opts).expect("shortcut overrides should apply");

        assert!(env
            .iter()
            .any(|(k, v)| k == "CLAWDEN_LLM_API_KEY" && v == "sk-test"));
        assert!(env
            .iter()
            .any(|(k, v)| k == "ZEROCLAW_LLM_API_KEY" && v == "sk-test"));
        assert!(env
            .iter()
            .any(|(k, v)| k == "CLAWDEN_API_KEY" && v == "sk-test"));
        assert!(env
            .iter()
            .any(|(k, v)| k == "OPENAI_API_KEY" && v == "sk-test"));
    }

    #[test]
    fn token_without_channel_returns_guidance_error() {
        let mut opts = make_opts();
        opts.token = Some("tg-test".to_string());
        let mut env = Vec::new();
        let err = apply_shortcut_env_overrides(&mut env, &opts)
            .expect_err("token without channel should fail with guidance");
        assert!(err.to_string().contains("Required fields for this run"));
        assert!(err.to_string().contains("--channel telegram"));
    }

    #[test]
    fn merge_env_overrides_last_occurrence_wins() {
        let merged = merge_env_overrides(
            vec![("A".to_string(), "base".to_string())],
            &[
                ("A".to_string(), "one".to_string()),
                ("A".to_string(), "two".to_string()),
            ],
        );
        assert_eq!(merged, vec![("A".to_string(), "two".to_string())]);
    }

    #[test]
    fn provider_and_model_overrides_flow_into_runtime_env() {
        let mut config = ClawDenYaml::parse_yaml(
            r#"
runtime: zeroclaw
provider: openai
model: gpt-4o-mini
providers:
  openai:
    api_key: sk-openai
"#,
        )
        .expect("yaml should parse");
        let mut opts = make_opts();
        opts.provider = Some("anthropic".to_string());
        opts.model = Some("claude-sonnet-4".to_string());
        opts.api_key = Some("sk-anthropic".to_string());

        apply_run_overrides(&mut config, &opts).expect("overrides should apply");
        config.resolve_env_vars().expect("env vars should resolve");
        let env = build_runtime_env_vars(&config, "zeroclaw").expect("env vars should build");

        assert!(env
            .iter()
            .any(|(k, v)| k == "CLAWDEN_LLM_PROVIDER" && v == "anthropic"));
        assert!(env
            .iter()
            .any(|(k, v)| k == "CLAWDEN_LLM_MODEL" && v == "claude-sonnet-4"));
        assert!(env
            .iter()
            .any(|(k, v)| k == "ANTHROPIC_API_KEY" && v == "sk-anthropic"));
    }

    #[test]
    fn system_prompt_override_is_written_into_config() {
        let mut config = ClawDenYaml::parse_yaml("runtime: zeroclaw\n").expect("yaml should parse");
        let mut opts = make_opts();
        opts.system_prompt = Some("You are a tutor".to_string());

        apply_run_overrides(&mut config, &opts).expect("overrides should apply");
        assert_eq!(
            config.config.get("system_prompt").and_then(|v| v.as_str()),
            Some("You are a tutor")
        );
    }

    #[test]
    fn system_prompt_file_is_loaded_for_env_override() {
        let mut opts = make_opts();
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let prompt_path = std::env::temp_dir().join(format!("clawden-prompt-{stamp}.txt"));
        std::fs::write(&prompt_path, "from-file").expect("prompt file should be written");
        opts.system_prompt = Some(format!("@{}", prompt_path.display()));

        let mut env = Vec::new();
        apply_shortcut_env_overrides(&mut env, &opts).expect("shortcut overrides should apply");
        assert!(env
            .iter()
            .any(|(k, v)| k == "CLAWDEN_SYSTEM_PROMPT" && v == "from-file"));
        let _ = std::fs::remove_file(prompt_path);
    }

    #[test]
    fn channel_shortcuts_map_tokens_app_tokens_and_phone() {
        let mut opts = make_opts();
        opts.channel = vec!["slack".to_string(), "signal".to_string()];
        opts.token = Some("bot-token".to_string());
        opts.app_token = Some("app-token".to_string());
        opts.phone = Some("+15551234567".to_string());

        let mut env = Vec::new();
        apply_shortcut_env_overrides(&mut env, &opts).expect("shortcut overrides should apply");
        assert!(env
            .iter()
            .any(|(k, v)| k == "SLACK_BOT_TOKEN" && v == "bot-token"));
        assert!(env
            .iter()
            .any(|(k, v)| k == "SIGNAL_BOT_TOKEN" && v == "bot-token"));
        assert!(env
            .iter()
            .any(|(k, v)| k == "SLACK_APP_TOKEN" && v == "app-token"));
        assert!(env
            .iter()
            .any(|(k, v)| k == "SIGNAL_PHONE" && v == "+15551234567"));
    }
}
