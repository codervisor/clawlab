use anyhow::Result;
use clawden_config::{ClawDenYaml, LlmProvider, ProviderEntryYaml, ProviderRefYaml};
use clawden_core::{ExecutionMode, LifecycleManager, ProcessManager, RuntimeInstaller};
use std::collections::HashMap;

use crate::commands::InitOptions;
use crate::util::{
    append_audit_file, ensure_installed, env_no_docker_enabled, is_first_run_context, parse_runtime,
    prompt_yes_no, get_provider_key_from_vault,
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
            anyhow::bail!("failed to resolve environment variables in clawden.yaml:\n{}", errs.join("\n"));
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
                let env_vars = if let Some(cfg) = config.as_ref() {
                    runtime_env_vars(cfg, &runtime)?
                } else {
                    Vec::new()
                };
                let info = process_manager.start_direct_with_env(&runtime, &executable, &[], &env_vars)?;
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

fn runtime_env_vars(config: &ClawDenYaml, runtime: &str) -> Result<Vec<(String, String)>> {
    let Some((provider_name, mut provider, model)) = runtime_provider_and_model(config, runtime) else {
        return Ok(Vec::new());
    };

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

    let mut env = HashMap::new();
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

            let provider = match config.provider.as_ref() {
                Some(ProviderRefYaml::Inline(entry)) => entry.clone(),
                Some(ProviderRefYaml::Name(name)) => config.providers.get(name).cloned().unwrap_or(ProviderEntryYaml {
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

fn provider_key_env_names(provider_type: Option<&LlmProvider>, provider_name: &str) -> Vec<&'static str> {
    let resolved = provider_type.cloned().or_else(|| infer_provider_type(provider_name));
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
    use super::{runtime_env_vars, ClawDenYaml};

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

        let env = runtime_env_vars(&config, "zeroclaw").expect("env vars should build");
        assert!(env.iter().any(|(k, v)| k == "OPENAI_API_KEY" && v == "sk-test"));
        assert!(env
            .iter()
            .any(|(k, v)| k == "CLAWDEN_LLM_MODEL" && v == "gpt-4o-mini"));
        assert!(env
            .iter()
            .any(|(k, v)| k == "ZEROCLAW_LLM_API_KEY" && v == "sk-test"));
    }
}
