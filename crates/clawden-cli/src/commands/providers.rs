use anyhow::Result;
use clawden_config::ClawDenYaml;
use reqwest::blocking::Client;
use std::collections::HashMap;
use std::io::{self, IsTerminal, Write};
use std::time::Duration;

use crate::cli::ProviderCommand;

pub fn exec_providers(command: Option<ProviderCommand>) -> Result<()> {
    match command {
        None => list_providers(),
        Some(ProviderCommand::Test { provider }) => test_providers(provider),
        Some(ProviderCommand::SetKey { provider }) => set_provider_key(&provider),
    }
}

fn list_providers() -> Result<()> {
    let yaml_path = std::env::current_dir()?.join("clawden.yaml");
    if !yaml_path.exists() {
        println!("No clawden.yaml found in current directory");
        return Ok(());
    }

    let mut config = ClawDenYaml::from_file(&yaml_path).map_err(anyhow::Error::msg)?;
    config.resolve_env_vars().map_err(|errs| anyhow::anyhow!(errs.join("\n")))?;

    if config.providers.is_empty() {
        println!("No providers configured in clawden.yaml");
        if let Some(provider) = config.provider {
            println!("single_runtime_provider={provider:?}");
        }
        return Ok(());
    }

    for (name, provider) in config.providers {
        let status = if provider.api_key.is_some() {
            "configured"
        } else {
            "missing_api_key"
        };
        println!("provider={name}\tstatus={status}");
    }
    Ok(())
}

fn test_providers(only: Option<String>) -> Result<()> {
    let yaml_path = std::env::current_dir()?.join("clawden.yaml");
    if !yaml_path.exists() {
        println!("No clawden.yaml found in current directory");
        return Ok(());
    }

    let mut config = ClawDenYaml::from_file(&yaml_path).map_err(anyhow::Error::msg)?;
    config.resolve_env_vars().map_err(|errs| anyhow::anyhow!(errs.join("\n")))?;

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
        match provider.api_key.as_deref() {
            Some(api_key) => match test_provider_endpoint(name, &base_url, api_key) {
                Ok(()) => println!("provider={name}\ttest=ok"),
                Err(err) => println!("provider={name}\ttest=fail\terror={err}"),
            },
            None => println!("provider={name}\ttest=fail\terror=missing api_key"),
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

fn test_provider_endpoint(provider: &str, base_url: &str, api_key: &str) -> Result<()> {
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

    let response = req.send()?;
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

    print!("Enter API key for {provider} (stored in .env as {env_name}): ");
    io::stdout().flush()?;
    let mut key = String::new();
    io::stdin().read_line(&mut key)?;
    let key = key.trim();
    if key.is_empty() {
        anyhow::bail!("API key cannot be empty");
    }

    let env_path = std::env::current_dir()?.join(".env");
    let mut entries = if env_path.exists() {
        parse_env_file(&std::fs::read_to_string(&env_path)?)
    } else {
        HashMap::new()
    };
    entries.insert(env_name.to_string(), key.to_string());

    let mut lines = vec!["# ClawDen environment variables".to_string()];
    let mut keys: Vec<_> = entries.keys().cloned().collect();
    keys.sort();
    for key in keys {
        if let Some(value) = entries.get(&key) {
            lines.push(format!("{key}={value}"));
        }
    }
    std::fs::write(&env_path, format!("{}\n", lines.join("\n")))?;

    println!("Stored key in {}", env_path.display());
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
