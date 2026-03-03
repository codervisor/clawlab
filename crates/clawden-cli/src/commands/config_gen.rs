use anyhow::Result;
use clawden_config::{ChannelCredentialMapper, ClawDenYaml};
use clawden_core::runtime_supported_extra_args;
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use toml::Value as TomlValue;

use super::up::{channels_for_runtime, runtime_provider_and_model};

pub(crate) fn generate_config_dir(
    config: &ClawDenYaml,
    runtime: &str,
    project_hash: &str,
) -> Result<Option<PathBuf>> {
    if !supports_config_dir(runtime) {
        return Ok(None);
    }

    let dir = runtime_config_dir(project_hash, runtime)?;
    fs::create_dir_all(&dir)?;

    match runtime {
        "zeroclaw" | "nullclaw" => {
            let body = generate_toml_config(config, runtime);
            fs::write(dir.join("config.toml"), toml::to_string_pretty(&body)?)?;
        }
        "picoclaw" => {
            let body = generate_picoclaw_config(config, runtime);
            fs::write(
                dir.join("config.json"),
                serde_json::to_string_pretty(&body)?,
            )?;
        }
        _ => {}
    }

    Ok(Some(dir))
}

pub(crate) fn inject_config_dir_arg(runtime: &str, args: &mut Vec<String>, config_dir: &Path) {
    if !supports_config_dir(runtime) {
        return;
    }
    args.push("--config-dir".to_string());
    args.push(config_dir.to_string_lossy().to_string());
}

pub(crate) fn cleanup_project_config_dir(project_hash: &str) -> Result<()> {
    let root = clawden_root_dir()?.join("configs").join(project_hash);
    if root.exists() {
        fs::remove_dir_all(root)?;
    }
    Ok(())
}

fn generate_toml_config(config: &ClawDenYaml, runtime: &str) -> toml::Table {
    let mut root = toml::Table::new();

    if let Some((provider_name, provider, model)) = runtime_provider_and_model(config, runtime) {
        root.insert(
            "default_provider".to_string(),
            TomlValue::String(provider_name.clone()),
        );
        if let Some(model_name) = model {
            root.insert("default_model".to_string(), TomlValue::String(model_name));
        }
        if let Some(api_key) = provider.api_key.filter(|v| !v.trim().is_empty()) {
            let mut api_key_row = toml::Table::new();
            api_key_row.insert("provider".to_string(), TomlValue::String(provider_name));
            api_key_row.insert("key".to_string(), TomlValue::String(api_key));
            let mut reliability = toml::Table::new();
            reliability.insert(
                "api_keys".to_string(),
                TomlValue::Array(vec![TomlValue::Table(api_key_row)]),
            );
            root.insert("reliability".to_string(), TomlValue::Table(reliability));
        }
    }

    let mut channels_cfg = toml::Table::new();
    for channel_name in channels_for_runtime(config, runtime) {
        let Some(channel) = config.channels.get(&channel_name) else {
            continue;
        };
        let channel_type = ClawDenYaml::resolve_channel_type(&channel_name, channel)
            .unwrap_or_else(|| channel_name.clone());
        let mut row = toml::Table::new();
        match channel_type.as_str() {
            "telegram" => {
                if let Some(token) = channel
                    .token
                    .as_ref()
                    .or(channel.bot_token.as_ref())
                    .filter(|v| !v.trim().is_empty())
                {
                    row.insert("bot_token".to_string(), TomlValue::String(token.clone()));
                }
                if !channel.allowed_users.is_empty() {
                    row.insert(
                        "allowed_users".to_string(),
                        TomlValue::Array(
                            channel
                                .allowed_users
                                .iter()
                                .cloned()
                                .map(TomlValue::String)
                                .collect(),
                        ),
                    );
                }
            }
            "discord" => {
                if let Some(token) = channel
                    .token
                    .as_ref()
                    .or(channel.bot_token.as_ref())
                    .filter(|v| !v.trim().is_empty())
                {
                    row.insert("bot_token".to_string(), TomlValue::String(token.clone()));
                }
                if let Some(guild_id) = channel.guild.as_ref().filter(|v| !v.trim().is_empty()) {
                    row.insert("guild_id".to_string(), TomlValue::String(guild_id.clone()));
                }
            }
            "slack" => {
                if let Some(bot_token) = channel.bot_token.as_ref().filter(|v| !v.trim().is_empty())
                {
                    row.insert(
                        "bot_token".to_string(),
                        TomlValue::String(bot_token.clone()),
                    );
                }
                if let Some(app_token) = channel.app_token.as_ref().filter(|v| !v.trim().is_empty())
                {
                    row.insert(
                        "app_token".to_string(),
                        TomlValue::String(app_token.clone()),
                    );
                }
            }
            "signal" => {
                if let Some(phone) = channel.phone.as_ref().filter(|v| !v.trim().is_empty()) {
                    row.insert("phone".to_string(), TomlValue::String(phone.clone()));
                }
                if let Some(token) = channel.token.as_ref().filter(|v| !v.trim().is_empty()) {
                    row.insert("token".to_string(), TomlValue::String(token.clone()));
                }
            }
            _ => {}
        }
        if !row.is_empty() {
            channels_cfg.insert(channel_type, TomlValue::Table(row));
        }
    }
    if !channels_cfg.is_empty() {
        root.insert(
            "channels_config".to_string(),
            TomlValue::Table(channels_cfg),
        );
    }

    merge_json_into_toml(&mut root, runtime_config_overrides(config, runtime));
    root
}

fn generate_picoclaw_config(
    config: &ClawDenYaml,
    runtime: &str,
) -> serde_json::Map<String, JsonValue> {
    let mut root = serde_json::Map::new();

    if let Some((provider_name, provider, model)) = runtime_provider_and_model(config, runtime) {
        let mut llm = serde_json::Map::new();
        llm.insert("provider".to_string(), JsonValue::String(provider_name));
        if let Some(model_name) = model {
            llm.insert("model".to_string(), JsonValue::String(model_name));
        }
        if let Some(api_key) = provider.api_key.filter(|v| !v.trim().is_empty()) {
            llm.insert("apiKeyRef".to_string(), JsonValue::String(api_key));
        }
        root.insert("llm".to_string(), JsonValue::Object(llm));
    }

    let mut channels = serde_json::Map::new();
    for channel_name in channels_for_runtime(config, runtime) {
        let Some(channel) = config.channels.get(&channel_name) else {
            continue;
        };
        let channel_type = ClawDenYaml::resolve_channel_type(&channel_name, channel)
            .unwrap_or_else(|| channel_name.clone());
        if let Ok(JsonValue::Object(obj)) =
            ChannelCredentialMapper::picoclaw_channel_config(&channel_type, channel)
        {
            channels.extend(obj);
        }
    }
    if !channels.is_empty() {
        root.insert("channels".to_string(), JsonValue::Object(channels));
    }

    for (k, v) in runtime_config_overrides(config, runtime) {
        root.insert(k.clone(), v.clone());
    }
    root
}

fn runtime_config_overrides<'a>(
    config: &'a ClawDenYaml,
    runtime: &str,
) -> &'a HashMap<String, JsonValue> {
    if config.runtime.as_deref() == Some(runtime) {
        return &config.config;
    }
    config
        .runtimes
        .iter()
        .find(|entry| entry.name == runtime)
        .map(|entry| &entry.config)
        .unwrap_or(&config.config)
}

fn merge_json_into_toml(target: &mut toml::Table, source: &HashMap<String, JsonValue>) {
    for (key, value) in source {
        if let Some(converted) = json_to_toml(value) {
            target.insert(key.clone(), converted);
        }
    }
}

fn json_to_toml(value: &JsonValue) -> Option<TomlValue> {
    match value {
        JsonValue::Null => None,
        JsonValue::Bool(v) => Some(TomlValue::Boolean(*v)),
        JsonValue::Number(v) => {
            if let Some(i) = v.as_i64() {
                Some(TomlValue::Integer(i))
            } else if let Some(f) = v.as_f64() {
                Some(TomlValue::Float(f))
            } else {
                None
            }
        }
        JsonValue::String(v) => Some(TomlValue::String(v.clone())),
        JsonValue::Array(items) => Some(TomlValue::Array(
            items.iter().filter_map(json_to_toml).collect(),
        )),
        JsonValue::Object(map) => {
            let mut out = toml::Table::new();
            for (k, v) in map {
                if let Some(converted) = json_to_toml(v) {
                    out.insert(k.clone(), converted);
                }
            }
            Some(TomlValue::Table(out))
        }
    }
}

fn supports_config_dir(runtime: &str) -> bool {
    runtime_supported_extra_args(runtime).contains(&"--config-dir")
}

fn clawden_root_dir() -> Result<PathBuf> {
    let home = std::env::var("HOME")?;
    Ok(PathBuf::from(home).join(".clawden"))
}

fn runtime_config_dir(project_hash: &str, runtime: &str) -> Result<PathBuf> {
    Ok(clawden_root_dir()?
        .join("configs")
        .join(project_hash)
        .join(runtime))
}

#[cfg(test)]
mod tests {
    use super::{cleanup_project_config_dir, generate_config_dir, inject_config_dir_arg};
    use crate::commands::test_env_lock;
    use clawden_config::ClawDenYaml;
    use std::fs;
    use std::path::Path;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn generates_zeroclaw_toml_with_channels_provider_and_overrides() {
        let _guard = test_env_lock().lock().expect("env lock");
        let original_home = std::env::var("HOME").ok();
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let tmp_home = std::env::temp_dir().join(format!("clawden-config-gen-{unique}"));
        fs::create_dir_all(&tmp_home).expect("tmp home");
        std::env::set_var("HOME", &tmp_home);

        let yaml = r#"
runtime: zeroclaw
provider: openrouter
model: anthropic/claude-sonnet-4-6
providers:
  openrouter:
    api_key: sk-or-test
channels:
  support-tg:
    type: telegram
    token: tg-test-token
    allowed_users: ["u1","u2"]
config:
  debug: true
"#;
        let mut config = ClawDenYaml::parse_yaml(yaml).expect("yaml parse");
        config.resolve_env_vars().expect("resolve env");

        let dir = generate_config_dir(&config, "zeroclaw", "abc123")
            .expect("config dir")
            .expect("supported runtime");
        let body = fs::read_to_string(dir.join("config.toml")).expect("read config");
        let parsed: toml::Value = body.parse().expect("valid toml");
        assert_eq!(
            parsed.get("default_provider").and_then(toml::Value::as_str),
            Some("openrouter")
        );
        assert_eq!(
            parsed.get("default_model").and_then(toml::Value::as_str),
            Some("anthropic/claude-sonnet-4-6")
        );
        assert_eq!(
            parsed
                .get("reliability")
                .and_then(|v| v.get("api_keys"))
                .and_then(toml::Value::as_array)
                .and_then(|rows| rows.first())
                .and_then(|row| row.get("provider"))
                .and_then(toml::Value::as_str),
            Some("openrouter")
        );
        assert_eq!(
            parsed
                .get("reliability")
                .and_then(|v| v.get("api_keys"))
                .and_then(toml::Value::as_array)
                .and_then(|rows| rows.first())
                .and_then(|row| row.get("key"))
                .and_then(toml::Value::as_str),
            Some("sk-or-test")
        );
        assert_eq!(
            parsed
                .get("channels_config")
                .and_then(|v| v.get("telegram"))
                .and_then(|v| v.get("bot_token"))
                .and_then(toml::Value::as_str),
            Some("tg-test-token")
        );
        assert_eq!(
            parsed
                .get("channels_config")
                .and_then(|v| v.get("telegram"))
                .and_then(|v| v.get("allowed_users"))
                .and_then(toml::Value::as_array)
                .map(Vec::len),
            Some(2)
        );
        assert_eq!(
            parsed.get("debug").and_then(toml::Value::as_bool),
            Some(true)
        );

        if let Some(home) = original_home {
            std::env::set_var("HOME", home);
        } else {
            std::env::remove_var("HOME");
        }
        let _ = fs::remove_dir_all(tmp_home);
    }

    #[test]
    fn injects_config_dir_only_for_supported_runtimes() {
        let mut zeroclaw_args = vec!["daemon".to_string()];
        inject_config_dir_arg("zeroclaw", &mut zeroclaw_args, Path::new("/tmp/cfg"));
        assert_eq!(
            zeroclaw_args,
            vec![
                "daemon".to_string(),
                "--config-dir".to_string(),
                "/tmp/cfg".to_string()
            ]
        );

        let mut openclaw_args = vec!["serve".to_string()];
        inject_config_dir_arg("openclaw", &mut openclaw_args, Path::new("/tmp/cfg"));
        assert_eq!(openclaw_args, vec!["serve".to_string()]);
    }

    #[test]
    fn cleanup_project_config_dir_removes_generated_tree() {
        let _guard = test_env_lock().lock().expect("env lock");
        let original_home = std::env::var("HOME").ok();
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let tmp_home = std::env::temp_dir().join(format!("clawden-config-cleanup-{unique}"));
        fs::create_dir_all(&tmp_home).expect("tmp home");
        std::env::set_var("HOME", &tmp_home);

        let yaml = r#"
runtime: zeroclaw
provider: openrouter
providers:
  openrouter:
    api_key: sk-or-test
"#;
        let mut config = ClawDenYaml::parse_yaml(yaml).expect("yaml parse");
        config.resolve_env_vars().expect("resolve env");
        let dir = generate_config_dir(&config, "zeroclaw", "cleanup-ph")
            .expect("config dir")
            .expect("supported runtime");
        assert!(dir.exists());

        cleanup_project_config_dir("cleanup-ph").expect("cleanup should succeed");
        assert!(!dir.exists());

        if let Some(home) = original_home {
            std::env::set_var("HOME", home);
        } else {
            std::env::remove_var("HOME");
        }
        let _ = fs::remove_dir_all(tmp_home);
    }

    #[test]
    fn project_config_dir_is_isolated_from_stale_home_config() {
        let _guard = test_env_lock().lock().expect("env lock");
        let original_home = std::env::var("HOME").ok();
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let tmp_home = std::env::temp_dir().join(format!("clawden-config-isolation-{unique}"));
        fs::create_dir_all(&tmp_home).expect("tmp home");
        std::env::set_var("HOME", &tmp_home);

        let stale_dir = tmp_home.join(".zeroclaw");
        fs::create_dir_all(&stale_dir).expect("stale dir");
        fs::write(
            stale_dir.join("config.toml"),
            "[channels_config.telegram]\nbot_token = \"stale-token\"\n",
        )
        .expect("stale config");

        let yaml = r#"
runtime: zeroclaw
channels:
  support-tg:
    type: telegram
    token: fresh-token
"#;
        let mut config = ClawDenYaml::parse_yaml(yaml).expect("yaml parse");
        config.resolve_env_vars().expect("resolve env");

        let generated_dir = generate_config_dir(&config, "zeroclaw", "iso-ph")
            .expect("config dir")
            .expect("supported runtime");
        let generated_body =
            fs::read_to_string(generated_dir.join("config.toml")).expect("generated config");
        let stale_body = fs::read_to_string(stale_dir.join("config.toml")).expect("stale config");

        let mut args = vec!["daemon".to_string()];
        inject_config_dir_arg("zeroclaw", &mut args, &generated_dir);

        assert!(args
            .windows(2)
            .any(|w| { w[0] == "--config-dir" && w[1] == generated_dir.to_string_lossy() }));
        assert!(generated_body.contains("fresh-token"));
        assert!(!generated_body.contains("stale-token"));
        assert!(stale_body.contains("stale-token"));

        if let Some(home) = original_home {
            std::env::set_var("HOME", home);
        } else {
            std::env::remove_var("HOME");
        }
        let _ = fs::remove_dir_all(tmp_home);
    }
}
