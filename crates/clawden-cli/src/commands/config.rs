use anyhow::Result;
use clawden_core::RuntimeInstaller;
use serde_json::json;

use crate::commands::config_gen::{
    generate_picoclaw_config, generate_toml_config, has_onboard_command, seed_template_config,
};
use crate::commands::up::load_config_with_env_file;

pub fn exec_config_show(
    runtime: &str,
    format: &str,
    reveal: bool,
    env_file: Option<&str>,
    installer: &RuntimeInstaller,
) -> Result<()> {
    // Default to the runtime-native config format for runtimes that use
    // --config-dir (zeroclaw, picoclaw, …).  The env-var view is still
    // available via --format native|env|json.
    let effective_format =
        if format == "native" && clawden_core::runtime_supports_config_dir(runtime) {
            "config"
        } else {
            format
        };

    if effective_format == "config" {
        return show_runtime_config(runtime, env_file, reveal, installer);
    }

    let env_vars = match load_config_with_env_file(env_file)? {
        Some(config) => super::up::build_runtime_env_vars(&config, runtime)?,
        None => {
            eprintln!("No clawden.yaml found — showing detected host environment for '{runtime}'");
            eprintln!("Tip: run `clawden init` to create a configuration file\n");
            detect_host_env_vars()
        }
    };

    match effective_format {
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
        _ => anyhow::bail!("unsupported format '{format}'. Use: native, env, json, config"),
    }

    Ok(())
}

/// Render the runtime-native config file (TOML or JSON) that would be written
/// to `--config-dir` during `clawden run`.  When the runtime supports
/// `onboard`, the template is seeded first so all required fields are present.
fn show_runtime_config(
    runtime: &str,
    env_file: Option<&str>,
    reveal: bool,
    installer: &RuntimeInstaller,
) -> Result<()> {
    if !clawden_core::runtime_supports_config_dir(runtime) {
        anyhow::bail!(
            "'{runtime}' does not use a config file (env-only runtime). \
             Use --format native|env|json instead."
        );
    }

    let config = load_config_with_env_file(env_file)?;
    let config = config.as_ref();

    let exe = installer.runtime_executable(runtime);

    match runtime {
        "zeroclaw" | "nullclaw" | "openfang" => {
            let base = if has_onboard_command(runtime) {
                // Seed into a temp dir so we don't pollute the real config dir.
                let unique = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_nanos())
                    .unwrap_or(0);
                let tmp = std::env::temp_dir().join(format!("clawden-config-show-{unique}"));
                std::fs::create_dir_all(&tmp).ok();
                let empty_cfg = empty_config(runtime);
                let cfg_ref = config.unwrap_or(&empty_cfg);
                let result = seed_template_config(exe.as_deref(), runtime, cfg_ref, &tmp);
                let _ = std::fs::remove_dir_all(&tmp);
                result
            } else {
                None
            };
            let body = if let Some(cfg) = config {
                generate_toml_config(cfg, runtime, base.as_ref())
            } else {
                base.unwrap_or_default()
            };
            let rendered = toml::to_string_pretty(&body)?;
            if reveal {
                println!("{rendered}");
            } else {
                println!("{}", redact_toml_secrets(&rendered));
            }
        }
        "picoclaw" => {
            if let Some(cfg) = config {
                let body = generate_picoclaw_config(cfg, runtime);
                let rendered = serde_json::to_string_pretty(&body)?;
                if reveal {
                    println!("{rendered}");
                } else {
                    println!("{}", redact_json_secrets(&rendered));
                }
            } else {
                println!("{{}}");
            }
        }
        _ => {
            anyhow::bail!("'{runtime}' config preview is not supported");
        }
    }
    Ok(())
}

fn empty_config(runtime: &str) -> clawden_config::ClawDenYaml {
    let yaml = format!("runtime: {runtime}\n");
    clawden_config::ClawDenYaml::parse_yaml(&yaml).expect("minimal yaml should parse")
}

/// Simple line-level redaction for TOML secrets — replaces string values of
/// keys that look like they contain secrets with `<redacted>`.  Only matches
/// lines whose value side is a quoted string (not integers, arrays, etc.).
fn redact_toml_secrets(toml_str: &str) -> String {
    toml_str
        .lines()
        .map(|line| {
            if let Some((key, val)) = line.split_once('=') {
                let val_trimmed = val.trim();
                // Only redact if the value is a quoted string.
                if val_trimmed.starts_with('"') && val_trimmed.ends_with('"') {
                    let key_lower = key.trim().to_ascii_lowercase();
                    if key_lower == "api_key"
                        || key_lower == "bot_token"
                        || key_lower == "app_token"
                        || key_lower.ends_with("_api_key")
                        || key_lower.ends_with("_bot_token")
                        || key_lower.ends_with("_app_token")
                    {
                        return format!("{} = \"<redacted>\"", key.trim_end());
                    }
                }
            }
            line.to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Simple line-level redaction for JSON secrets.
fn redact_json_secrets(json_str: &str) -> String {
    json_str
        .lines()
        .map(|line| {
            if let Some((key_part, val_part)) = line.split_once(':') {
                let val_trimmed = val_part.trim().trim_end_matches(',');
                // Only redact if the value is a quoted string.
                if val_trimmed.starts_with('"') && val_trimmed.ends_with('"') {
                    let key_lower = key_part.trim().trim_matches('"').to_ascii_lowercase();
                    if key_lower == "api_key"
                        || key_lower == "apikey"
                        || key_lower == "apikeyref"
                        || key_lower == "bot_token"
                        || key_lower == "app_token"
                    {
                        let indent = &line[..line.len() - line.trim_start().len()];
                        let key_trimmed = key_part.trim();
                        let comma = if line.trim_end().ends_with(',') {
                            ","
                        } else {
                            ""
                        };
                        return format!("{indent}{key_trimmed}: \"<redacted>\"{comma}");
                    }
                }
            }
            line.to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
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
