use anyhow::Result;
use clawden_config::{
    ChannelCredentialMapper, ClawDenYaml, LlmProvider, ProviderEntryYaml, ProviderRefYaml,
};
use clawden_core::{ExecutionMode, LifecycleManager, ProcessManager, RuntimeInstaller};
use std::collections::HashMap;

use crate::commands::InitOptions;
use crate::util::{
    append_audit_file, ensure_installed, env_no_docker_enabled, get_provider_key_from_vault,
    is_first_run_context, parse_runtime, prompt_yes_no,
};

pub async fn exec_up(
    runtimes: Vec<String>,
    no_docker: bool,
    installer: &RuntimeInstaller,
    process_manager: &ProcessManager,
    manager: &mut LifecycleManager,
) -> Result<()> {
    if runtimes.is_empty() && is_first_run_context(installer)? {
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

    let yaml_path = std::env::current_dir()?.join("clawden.yaml");
    let config = if yaml_path.exists() {
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
        Some(cfg)
    } else {
        None
    };

    // Determine target runtimes: CLI args > clawden.yaml > installed runtimes
    let target_runtimes = if !runtimes.is_empty() {
        runtimes
    } else if let Some(cfg) = config.as_ref() {
        let from_config = runtimes_from_config(cfg);
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
    };

    if target_runtimes.is_empty() {
        println!("No runtimes to start. Create a clawden.yaml with 'clawden init' or install one with 'clawden install zeroclaw'");
        return Ok(());
    }

    let mut started_runtimes = Vec::new();
    let mut started_pids = Vec::new();

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
                let executable = ensure_installed(installer, &runtime)?;
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

                let mut args = vec!["daemon".to_string()];
                if !channels.is_empty() {
                    args.push(format!("--channels={}", channels.join(",")));
                }

                let info = process_manager.start_direct_with_env(
                    &runtime,
                    &executable,
                    &args,
                    &env_vars,
                )?;
                append_audit_file("runtime.start", &runtime, "ok")?;
                println!("Started {runtime} (pid {})", info.pid);
                started_runtimes.push(runtime.clone());
                started_pids.push(info.pid);
            }
        }
    }

    if !started_runtimes.is_empty() {
        println!("All runtimes started. Press Ctrl+C to stop.");
        wait_for_shutdown_or_exit(&started_pids).await;
        println!("Shutting down...");
        for runtime in &started_runtimes {
            if let Err(e) = process_manager.stop(runtime) {
                eprintln!("Warning: failed to stop {runtime}: {e}");
            }
        }
    }

    Ok(())
}

/// Wait until Ctrl+C is received or all runtime processes have exited.
async fn wait_for_shutdown_or_exit(pids: &[u32]) {
    let ctrl_c = tokio::signal::ctrl_c();
    tokio::pin!(ctrl_c);

    let mut check_interval = tokio::time::interval(std::time::Duration::from_secs(2));
    // Skip the first immediate tick
    check_interval.tick().await;

    loop {
        tokio::select! {
            _ = &mut ctrl_c => break,
            _ = check_interval.tick() => {
                if pids.iter().all(|pid| !is_process_alive(*pid)) {
                    break;
                }
            }
        }
    }
}

/// Check if a process is alive (not a zombie) by inspecting /proc/<pid>/status.
fn is_process_alive(pid: u32) -> bool {
    let status_path = format!("/proc/{pid}/status");
    match std::fs::read_to_string(status_path) {
        Ok(content) => {
            // A zombie process has "State:\tZ" — treat zombies as dead.
            !content
                .lines()
                .any(|line| line.starts_with("State:") && line.contains('Z'))
        }
        Err(_) => false, // process no longer exists
    }
}

/// Extract runtime names from a parsed clawden.yaml config.
fn runtimes_from_config(config: &ClawDenYaml) -> Vec<String> {
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
