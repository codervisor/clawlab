use anyhow::Result;
use clawden_config::ClawDenYaml;
use reqwest::Client;
use std::collections::HashMap;
use std::io::{self, IsTerminal, Write};
use std::time::Duration;

use crate::cli::ProviderCommand;
use crate::util::{get_provider_key_from_vault, store_provider_key_in_vault};

pub async fn exec_providers(command: Option<ProviderCommand>) -> Result<()> {
    match command {
        None => list_providers(),
        Some(ProviderCommand::Test { provider }) => test_providers(provider).await,
        Some(ProviderCommand::SetKey { provider }) => set_provider_key(&provider),
    }
}

fn list_providers() -> Result<()> {
    let yaml_path = std::env::current_dir()?.join("clawden.yaml");
    if !yaml_path.exists() {
        return list_detected_providers();
    }

    let mut config = ClawDenYaml::from_file(&yaml_path).map_err(anyhow::Error::msg)?;
    config
        .resolve_env_vars()
        .map_err(|errs| anyhow::anyhow!(errs.join("\n")))?;

    if config.providers.is_empty() {
        println!("No providers configured in clawden.yaml");
        if let Some(provider) = config.provider {
            println!("single_runtime_provider={provider:?}");
        }
        return Ok(());
    }

    for (name, provider) in config.providers {
        let status = if provider.api_key.is_some() || get_provider_key_from_vault(&name)?.is_some()
        {
            "configured"
        } else {
            "missing_api_key"
        };
        println!("provider={name}\tstatus={status}");
    }
    Ok(())
}

fn list_detected_providers() -> Result<()> {
    eprintln!("No clawden.yaml found — showing providers detected from environment\n");

    let known_providers: &[(&str, &str)] = &[
        ("openrouter", "OPENROUTER_API_KEY"),
        ("openai", "OPENAI_API_KEY"),
        ("anthropic", "ANTHROPIC_API_KEY"),
        ("google", "GEMINI_API_KEY"),
        ("mistral", "MISTRAL_API_KEY"),
        ("groq", "GROQ_API_KEY"),
    ];

    let mut found = false;
    for (name, env_var) in known_providers {
        let from_env = std::env::var(env_var).ok().filter(|v| !v.trim().is_empty());
        let from_vault = get_provider_key_from_vault(name)?.filter(|v| !v.trim().is_empty());

        if from_env.is_some() || from_vault.is_some() {
            let source = if from_env.is_some() { "env" } else { "vault" };
            println!("provider={name}\tstatus=detected\tsource={source}");
            found = true;
        }
    }

    if !found {
        println!("No provider API keys detected in environment or vault");
        eprintln!("\nTip: set a provider key (e.g. OPENROUTER_API_KEY) or run `clawden init`");
    }
    Ok(())
}

async fn test_providers(only: Option<String>) -> Result<()> {
    let yaml_path = std::env::current_dir()?.join("clawden.yaml");
    if !yaml_path.exists() {
        return test_detected_providers(only).await;
    }

    let mut config = ClawDenYaml::from_file(&yaml_path).map_err(anyhow::Error::msg)?;
    config
        .resolve_env_vars()
        .map_err(|errs| anyhow::anyhow!(errs.join("\n")))?;

    let mut any = false;
    for (name, provider) in &config.providers {
        if let Some(target) = &only {
            if target != name {
                continue;
            }
        }
        any = true;
        let base_url = provider
            .base_url
            .clone()
            .unwrap_or_else(|| "https://api.openai.com/v1".to_string());
        let api_key = provider
            .api_key
            .clone()
            .or(get_provider_key_from_vault(name)?)
            .unwrap_or_default();
        if api_key.is_empty() {
            println!("provider={name}\ttest=fail\terror=missing api_key");
            continue;
        }
        match test_provider_endpoint(name, &base_url, &api_key).await {
            Ok(()) => println!("provider={name}\ttest=ok"),
            Err(err) => println!("provider={name}\ttest=fail\terror={err}"),
        }
    }

    if !any {
        if let Some(target) = only {
            println!("No matching provider '{target}' found in clawden.yaml");
        } else {
            println!("No providers configured in clawden.yaml");
        }
    }

    Ok(())
}

async fn test_detected_providers(only: Option<String>) -> Result<()> {
    eprintln!("No clawden.yaml found — testing providers detected from environment\n");

    let known_providers: &[(&str, &str, &str)] = &[
        (
            "openrouter",
            "OPENROUTER_API_KEY",
            "https://openrouter.ai/api/v1",
        ),
        ("openai", "OPENAI_API_KEY", "https://api.openai.com/v1"),
        (
            "anthropic",
            "ANTHROPIC_API_KEY",
            "https://api.anthropic.com",
        ),
        (
            "google",
            "GEMINI_API_KEY",
            "https://generativelanguage.googleapis.com/v1beta",
        ),
        ("mistral", "MISTRAL_API_KEY", "https://api.mistral.ai/v1"),
        ("groq", "GROQ_API_KEY", "https://api.groq.com/openai/v1"),
    ];

    let mut any = false;
    for (name, env_var, base_url) in known_providers {
        if let Some(target) = &only {
            if target != name {
                continue;
            }
        }
        let api_key = std::env::var(env_var)
            .ok()
            .filter(|v| !v.trim().is_empty())
            .or_else(|| get_provider_key_from_vault(name).ok().flatten());

        let Some(api_key) = api_key else {
            continue;
        };

        any = true;
        match test_provider_endpoint(name, base_url, &api_key).await {
            Ok(()) => println!("provider={name}\ttest=ok\tsource=env"),
            Err(err) => println!("provider={name}\ttest=fail\terror={err}"),
        }
    }

    if !any {
        if let Some(target) = only {
            println!("No API key detected for provider '{target}'");
            if let Some(env_var) = provider_env_var(&target) {
                eprintln!("\nTip: set {env_var} or run `clawden providers set-key {target}`");
            }
        } else {
            println!("No provider API keys detected in environment or vault");
            eprintln!("\nTip: set a provider key (e.g. OPENROUTER_API_KEY) or run `clawden init`");
        }
    }

    Ok(())
}

async fn test_provider_endpoint(provider: &str, base_url: &str, api_key: &str) -> Result<()> {
    let endpoint = if provider == "anthropic" {
        format!("{}/v1/models", base_url.trim_end_matches('/'))
    } else {
        format!("{}/models", base_url.trim_end_matches('/'))
    };

    let client = Client::builder().timeout(Duration::from_secs(8)).build()?;
    let mut req = client.get(endpoint);
    match provider {
        "anthropic" => {
            req = req
                .header("x-api-key", api_key)
                .header("anthropic-version", "2023-06-01");
        }
        _ => {
            req = req.bearer_auth(api_key);
        }
    }

    let response = req.send().await?;
    if response.status().is_success() {
        Ok(())
    } else {
        anyhow::bail!("http_status={}", response.status());
    }
}

fn set_provider_key(provider: &str) -> Result<()> {
    if !io::stdin().is_terminal() {
        anyhow::bail!("set-key requires an interactive terminal");
    }

    let env_name = provider_env_var(provider)
        .ok_or_else(|| anyhow::anyhow!("unknown provider '{provider}'"))?;

    print!(
        "Enter API key for {provider} (stored in local vault; .env keeps placeholder {env_name}): "
    );
    io::stdout().flush()?;
    let key = rpassword::read_password()?.trim().to_string();
    if key.is_empty() {
        anyhow::bail!("API key cannot be empty");
    }

    let vault_path = store_provider_key_in_vault(provider, &key)?;

    let env_path = std::env::current_dir()?.join(".env");
    let mut entries = if env_path.exists() {
        parse_env_file(&std::fs::read_to_string(&env_path)?)
    } else {
        HashMap::new()
    };
    entries
        .entry(env_name.to_string())
        .or_insert_with(String::new);

    let mut lines = vec!["# ClawDen environment variables".to_string()];
    let mut keys: Vec<_> = entries.keys().cloned().collect();
    keys.sort();
    for key in keys {
        if let Some(value) = entries.get(&key) {
            lines.push(format!("{key}={value}"));
        }
    }
    std::fs::write(&env_path, format!("{}\n", lines.join("\n")))?;

    println!("Stored encrypted key in {}", vault_path.display());
    println!("Ensured placeholder in {}", env_path.display());
    Ok(())
}

fn provider_env_var(provider: &str) -> Option<&'static str> {
    match provider {
        "openai" => Some("OPENAI_API_KEY"),
        "anthropic" => Some("ANTHROPIC_API_KEY"),
        "google" => Some("GEMINI_API_KEY"),
        "openrouter" => Some("OPENROUTER_API_KEY"),
        "mistral" => Some("MISTRAL_API_KEY"),
        "groq" => Some("GROQ_API_KEY"),
        _ => None,
    }
}

fn parse_env_file(content: &str) -> HashMap<String, String> {
    let mut values = HashMap::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = trimmed.split_once('=') {
            values.insert(key.trim().to_string(), value.trim().to_string());
        }
    }
    values
}
