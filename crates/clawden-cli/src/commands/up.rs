use anyhow::Result;
use clawden_config::{
    ChannelCredentialMapper, ClawDenYaml, LlmProvider, ProviderEntryYaml, ProviderRefYaml,
};
use clawden_core::{AgentState, ExecutionMode, LifecycleManager, ProcessManager, RuntimeInstaller};
use std::collections::HashMap;
use std::time::Duration;

use crate::commands::InitOptions;
use crate::util::{
    append_audit_file, ensure_installed_runtime, env_no_docker_enabled,
    get_provider_key_from_vault, is_first_run_context, parse_runtime, project_hash, prompt_yes_no,
};

pub struct UpOptions {
    pub runtimes: Vec<String>,
    pub detach: bool,
    pub no_log_prefix: bool,
    pub timeout: u64,
}

pub async fn exec_up(
    opts: UpOptions,
    no_docker: bool,
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
                reconfigure: false,
                non_interactive: false,
                yes: false,
                force: false,
            })?;
        } else {
            println!("Setup skipped.");
            return Ok(());
        }
    }

    let mode = process_manager.resolve_mode(no_docker || env_no_docker_enabled());
    let config = load_config()?;
    let target_runtimes =
        resolve_target_runtimes(opts.runtimes.clone(), config.as_ref(), installer)?;

    if target_runtimes.is_empty() {
        println!("No runtimes to start. Create a clawden.yaml with 'clawden init' or install one with 'clawden install zeroclaw'");
        return Ok(());
    }

    let mut started_runtimes = Vec::new();

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
                started_runtimes.push(runtime.clone());
            }
            ExecutionMode::Direct | ExecutionMode::Auto => {
                let installed = ensure_installed_runtime(installer, &runtime)?;
                let env_vars = if let Some(cfg) = config.as_ref() {
                    build_runtime_env_vars(cfg, &runtime)?
                } else {
                    Vec::new()
                };

                let channels = if let Some(cfg) = config.as_ref() {
                    channels_for_runtime(cfg, &runtime)
                } else {
                    Vec::new()
                };

                let mut args = installed.start_args.clone();
                if !channels.is_empty() {
                    args.push(format!("--channels={}", channels.join(",")));
                }

                let info = process_manager.start_direct_with_env_and_project(
                    &runtime,
                    &installed.executable,
                    &args,
                    &env_vars,
                    Some(project_hash()?),
                )?;
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
    let yaml_path = std::env::current_dir()?.join("clawden.yaml");
    if !yaml_path.exists() {
        return Ok(None);
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
fn channels_for_runtime(config: &ClawDenYaml, runtime: &str) -> Vec<String> {
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

            // Use runtime-specific env var mappers where available
            let channel_vars = match runtime_slug.as_str() {
                "zeroclaw" => ChannelCredentialMapper::zeroclaw_env_vars(&ch_type, ch_instance),
                "nanoclaw" => ChannelCredentialMapper::nanoclaw_env_vars(&ch_type, ch_instance),
                _ => ChannelCredentialMapper::zeroclaw_env_vars(&ch_type, ch_instance),
            };
            env.extend(channel_vars);
        }
    }

    let mut pairs: Vec<_> = env.into_iter().collect();
    pairs.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(pairs)
}

fn runtime_provider_and_model(
    config: &ClawDenYaml,
    runtime: &str,
) -> Option<(String, ProviderEntryYaml, Option<String>)> {
    if let Some(single_runtime) = &config.runtime {
        if single_runtime == runtime {
            let provider_name = match config.provider.as_ref() {
                Some(ProviderRefYaml::Name(name)) => name.clone(),
                Some(ProviderRefYaml::Inline(_)) => "provider".to_string(),
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

fn infer_provider_type(name: &str) -> Option<LlmProvider> {
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
    use super::{build_runtime_env_vars, ClawDenYaml};

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
}
