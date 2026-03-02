use anyhow::Result;
use clawden_config::{ChannelInstanceYaml, ClawDenYaml};
use clawden_core::LifecycleManager;

use crate::cli::ChannelCommand;

pub fn exec_channels(
    command: Option<ChannelCommand>,
    manager: &mut LifecycleManager,
) -> Result<()> {
    match command {
        None => {
            let metadata = manager.list_runtime_metadata();
            for runtime in metadata {
                println!("{}", runtime.runtime.as_slug());
                for (channel, support) in runtime.channel_support {
                    println!("  {}: {:?}", channel, support);
                }
            }
        }
        Some(ChannelCommand::Test { channel_type }) => {
            test_channels(channel_type.as_deref())?;
        }
    }
    Ok(())
}

fn test_channels(filter_type: Option<&str>) -> Result<()> {
    let yaml_path = std::env::current_dir()?.join("clawden.yaml");
    if !yaml_path.exists() {
        println!("No clawden.yaml found in current directory");
        return Ok(());
    }

    let mut config = ClawDenYaml::from_file(&yaml_path).map_err(anyhow::Error::msg)?;
    config
        .resolve_env_vars()
        .map_err(|errs| anyhow::anyhow!(errs.join("\n")))?;

    if config.channels.is_empty() {
        println!("No channels configured in clawden.yaml");
        return Ok(());
    }

    let mut tested = 0usize;
    let mut failed = 0usize;

    for (name, ch) in &config.channels {
        let channel_type =
            ClawDenYaml::resolve_channel_type(name, ch).unwrap_or_else(|| "unknown".to_string());

        if let Some(filter) = filter_type {
            if channel_type != filter {
                continue;
            }
        }

        tested += 1;
        let errors = validate_channel(channel_type.as_str(), ch);
        if errors.is_empty() {
            println!("channel={name}\ttype={channel_type}\ttest=ok");
        } else {
            failed += 1;
            println!(
                "channel={name}\ttype={channel_type}\ttest=fail\terrors={}",
                errors.join("; ")
            );
        }
    }

    if tested == 0 {
        if let Some(filter) = filter_type {
            println!("No channels of type '{filter}' configured");
        } else {
            println!("No channels configured");
        }
        return Ok(());
    }

    if failed > 0 {
        anyhow::bail!("{failed}/{tested} channel config(s) failed validation");
    }

    println!("All {tested} channel config(s) passed validation");
    Ok(())
}

fn validate_channel(channel_type: &str, ch: &ChannelInstanceYaml) -> Vec<String> {
    let mut errors = Vec::new();
    let has = |opt: &Option<String>| opt.as_deref().is_some_and(|v| !v.trim().is_empty());

    match channel_type {
        "telegram" | "discord" | "feishu" | "lark" => {
            if !has(&ch.token) {
                errors.push("missing token".to_string());
            }
        }
        "slack" => {
            if !has(&ch.bot_token) {
                errors.push("missing bot_token".to_string());
            }
            if !has(&ch.app_token) {
                errors.push("missing app_token".to_string());
            }
        }
        "whatsapp" => {
            if !has(&ch.token) && !has(&ch.phone) {
                errors.push("missing token or phone".to_string());
            }
        }
        "signal" => {
            if !has(&ch.phone) {
                errors.push("missing phone".to_string());
            }
        }
        "dingtalk" => {
            let app_id = ch.extra.get("app_id").and_then(serde_json::Value::as_str);
            let app_secret = ch
                .extra
                .get("app_secret")
                .and_then(serde_json::Value::as_str);
            if app_id.is_none_or(str::is_empty) {
                errors.push("missing app_id".to_string());
            }
            if app_secret.is_none_or(str::is_empty) {
                errors.push("missing app_secret".to_string());
            }
        }
        "qq" => {
            let uin = ch.extra.get("uin").and_then(serde_json::Value::as_str);
            if uin.is_none_or(str::is_empty) && !has(&ch.token) {
                errors.push("missing uin or token".to_string());
            }
        }
        _ => {
            if !has(&ch.token) {
                errors.push("missing token".to_string());
            }
        }
    }

    errors
}
