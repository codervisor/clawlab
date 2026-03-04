use anyhow::Result;
use serde_json::json;

use crate::commands::up::load_config_with_env_file;

pub fn exec_config_show(
    runtime: &str,
    format: &str,
    reveal: bool,
    env_file: Option<&str>,
) -> Result<()> {
    let env_vars = match load_config_with_env_file(env_file)? {
        Some(config) => super::up::build_runtime_env_vars(&config, runtime)?,
        None => {
            eprintln!("No clawden.yaml found — showing detected host environment for '{runtime}'");
            eprintln!("Tip: run `clawden init` to create a configuration file\n");
            detect_host_env_vars()
        }
    };

    match format {
        "native" => {
            println!("[runtime]");
            println!("name = \"{runtime}\"");
            println!("\n[env]");
            for (k, v) in env_vars {
                println!("{k} = \"{}\"", maybe_redact(&k, &v, reveal));
            }
        }
        "env" => {
            for (k, v) in env_vars {
                println!("{k}={}", maybe_redact(&k, &v, reveal));
            }
        }
        "json" => {
            let env = env_vars
                .into_iter()
                .map(|(k, v)| (k.clone(), json!(maybe_redact(&k, &v, reveal))))
                .collect::<serde_json::Map<_, _>>();
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({"runtime": runtime, "env": env}))?
            );
        }
        _ => anyhow::bail!("unsupported format '{format}'. Use: native, env, json"),
    }

    Ok(())
}

pub fn exec_config_env(reveal: bool) -> Result<()> {
    let provider_vars: &[(&str, &str)] = &[
        ("OPENROUTER_API_KEY", "openrouter"),
        ("OPENAI_API_KEY", "openai"),
        ("ANTHROPIC_API_KEY", "anthropic"),
        ("GEMINI_API_KEY", "google"),
        ("GOOGLE_API_KEY", "google"),
        ("MISTRAL_API_KEY", "mistral"),
        ("GROQ_API_KEY", "groq"),
    ];

    let channel_vars: &[&str] = &[
        "TELEGRAM_BOT_TOKEN",
        "DISCORD_BOT_TOKEN",
        "SLACK_BOT_TOKEN",
        "SLACK_APP_TOKEN",
    ];

    let config_vars: &[&str] = &[
        "CLAWDEN_LLM_API_KEY",
        "CLAWDEN_LLM_PROVIDER",
        "CLAWDEN_LLM_MODEL",
        "CLAWDEN_LLM_BASE_URL",
    ];

    println!("Detected environment variables:");

    println!("\n  LLM Providers:");
    for (var_name, provider) in provider_vars {
        print_env_status(var_name, Some(provider), reveal);
    }

    println!("\n  Channel Tokens:");
    for var_name in channel_vars {
        print_env_status(var_name, None, reveal);
    }

    println!("\n  ClawDen Config:");
    for var_name in config_vars {
        print_env_status(var_name, None, reveal);
    }

    Ok(())
}

fn print_env_status(var_name: &str, label: Option<&str>, reveal: bool) {
    let dots = ".".repeat(26usize.saturating_sub(var_name.len()));
    match std::env::var(var_name) {
        Ok(val) if !val.trim().is_empty() => {
            let display = if reveal { val } else { redact_value(&val) };
            let suffix = label.map(|l| format!(" ({l})")).unwrap_or_default();
            println!("    {var_name} {dots} \u{2713} set ({display}){suffix}");
        }
        _ => {
            println!("    {var_name} {dots} \u{2717} not set");
        }
    }
}

fn redact_value(value: &str) -> String {
    if value.len() <= 8 {
        "***".to_string()
    } else {
        format!("{}***", &value[..8])
    }
}

fn maybe_redact(key: &str, value: &str, reveal: bool) -> String {
    if reveal {
        return value.to_string();
    }
    let upper = key.to_ascii_uppercase();
    if upper.contains("TOKEN") || upper.contains("KEY") || upper.contains("SECRET") {
        return "<redacted>".to_string();
    }
    value.to_string()
}

/// Scan the host environment for known ClawDen-relevant variables.
fn detect_host_env_vars() -> Vec<(String, String)> {
    let known_vars: &[&str] = &[
        "CLAWDEN_LLM_API_KEY",
        "CLAWDEN_LLM_PROVIDER",
        "CLAWDEN_LLM_MODEL",
        "CLAWDEN_LLM_BASE_URL",
        "OPENROUTER_API_KEY",
        "OPENAI_API_KEY",
        "ANTHROPIC_API_KEY",
        "GEMINI_API_KEY",
        "GOOGLE_API_KEY",
        "MISTRAL_API_KEY",
        "GROQ_API_KEY",
        "TELEGRAM_BOT_TOKEN",
        "DISCORD_BOT_TOKEN",
        "SLACK_BOT_TOKEN",
        "SLACK_APP_TOKEN",
    ];

    let mut pairs: Vec<(String, String)> = known_vars
        .iter()
        .filter_map(|&name| {
            std::env::var(name)
                .ok()
                .filter(|v| !v.trim().is_empty())
                .map(|v| (name.to_string(), v))
        })
        .collect();
    pairs.sort_by(|a, b| a.0.cmp(&b.0));
    pairs
}
