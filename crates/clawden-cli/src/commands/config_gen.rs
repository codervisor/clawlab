use anyhow::Result;
use clawden_config::{ChannelCredentialMapper, ClawDenYaml};
use clawden_core::runtime_supports_config_dir;
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::fs;
#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use toml::Value as TomlValue;
use tracing::debug;

use super::up::{channels_for_runtime, runtime_provider_and_model};

pub(crate) fn generate_config_dir(
    config: &ClawDenYaml,
    runtime: &str,
    project_hash: &str,
    executable: Option<&Path>,
) -> Result<Option<PathBuf>> {
    if !supports_config_dir(runtime) {
        return Ok(None);
    }

    let dir = runtime_config_dir(project_hash, runtime)?;
    fs::create_dir_all(&dir)?;

    match runtime {
        "zeroclaw" | "nullclaw" | "openfang" => {
            let base = if has_onboard_command(runtime) {
                seed_template_config(executable, runtime, config, &dir)
            } else {
                None
            };
            let body = generate_toml_config(config, runtime, base.as_ref());
            write_secret_file(
                &dir.join("config.toml"),
                toml::to_string_pretty(&body)?.as_bytes(),
            )?;
        }
        "picoclaw" => {
            let body = generate_picoclaw_config(config, runtime);
            write_secret_file(
                &dir.join("config.json"),
                serde_json::to_string_pretty(&body)?.as_bytes(),
            )?;
        }
        _ => {}
    }

    Ok(Some(dir))
}

/// Returns true for runtimes that support `<runtime> onboard --config-dir`
/// to generate a template config with all required default fields.
pub(crate) fn has_onboard_command(runtime: &str) -> bool {
    matches!(runtime, "zeroclaw")
}

/// Run `<runtime> onboard --config-dir <dir> --force` to seed a template
/// config.toml that contains all required fields with sensible defaults.
/// Returns the parsed TOML table on success, or None if the executable is
/// unavailable or onboard fails (we fall back to the old behaviour).
pub(crate) fn seed_template_config(
    executable: Option<&Path>,
    runtime: &str,
    config: &ClawDenYaml,
    config_dir: &Path,
) -> Option<toml::Table> {
    let exe = executable?;
    if !exe.exists() {
        return None;
    }

    let mut cmd = Command::new(exe);
    cmd.arg("onboard")
        .arg("--config-dir")
        .arg(config_dir)
        .arg("--force")
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    // Pass provider hint so onboard writes a matching default_provider.
    if let Some((provider_name, _, _)) = runtime_provider_and_model(config, runtime) {
        cmd.arg("--provider").arg(&provider_name);
    }

    debug!(
        "seeding template config via {} onboard --config-dir {}",
        exe.display(),
        config_dir.display()
    );

    match cmd.status() {
        Ok(status) if status.success() => {
            let toml_path = config_dir.join("config.toml");
            let content = fs::read_to_string(&toml_path).ok()?;
            let table: toml::Table = content.parse().ok()?;

            // onboard creates workspace scaffolding (sessions/, memory/,
            // IDENTITY.md, …) that we don't need — remove everything except
            // config.toml itself.
            cleanup_onboard_artifacts(config_dir);

            Some(table)
        }
        Ok(status) => {
            debug!(
                "{} onboard exited with status {} — falling back to generated config",
                runtime, status
            );
            None
        }
        Err(e) => {
            debug!(
                "failed to run {} onboard: {} — falling back to generated config",
                runtime, e
            );
            None
        }
    }
}

/// Remove workspace artefacts that `onboard` creates alongside config.toml.
fn cleanup_onboard_artifacts(config_dir: &Path) {
    // Keep only config.toml (and config.json for picoclaw).
    let keep: &[&str] = &["config.toml", "config.json"];
    if let Ok(entries) = fs::read_dir(config_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if !keep.contains(&name_str.as_ref()) {
                let path = entry.path();
                if path.is_dir() {
                    let _ = fs::remove_dir_all(&path);
                } else {
                    let _ = fs::remove_file(&path);
                }
            }
        }
    }
}

pub(crate) fn inject_config_dir_arg(runtime: &str, args: &mut Vec<String>, config_dir: &Path) {
    if !supports_config_dir(runtime) {
        return;
    }
    let insert_at = args
        .iter()
        .position(|arg| !arg.starts_with('-'))
        .map(|idx| idx + 1)
        .unwrap_or(0);
    args.insert(insert_at, "--config-dir".to_string());
    args.insert(insert_at + 1, config_dir.to_string_lossy().to_string());
}

pub(crate) fn cleanup_project_config_dir(project_hash: &str) -> Result<()> {
    let root = clawden_root_dir()?.join("configs").join(project_hash);
    if root.exists() {
        fs::remove_dir_all(root)?;
    }
    Ok(())
}

pub(crate) fn generate_toml_config(
    config: &ClawDenYaml,
    runtime: &str,
    base: Option<&toml::Table>,
) -> toml::Table {
    // Start from the template if available — this preserves all required
    // default fields that the runtime expects (e.g. default_temperature).
    let mut root = base.cloned().unwrap_or_default();

    if let Some((provider_name, provider, model)) = runtime_provider_and_model(config, runtime) {
        root.insert(
            "default_provider".to_string(),
            TomlValue::String(provider_name.clone()),
        );
        if let Some(model_name) = model {
            root.insert("default_model".to_string(), TomlValue::String(model_name));
        }
        if let Some(api_key) = provider.api_key.filter(|v| !v.trim().is_empty()) {
            let mut reliability = toml::Table::new();
            reliability.insert(
                "api_keys".to_string(),
                TomlValue::Array(vec![TomlValue::String(api_key)]),
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
                // zeroclaw requires `allowed_users` to always be present
                // (empty = deny all, which is the safe default).
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

pub(crate) fn generate_picoclaw_config(
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
        let key = if k == "system_prompt" {
            // picoclaw expects camelCase for this field.
            "systemPrompt".to_string()
        } else {
            k.clone()
        };
        root.insert(key, v.clone());
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
            } else {
                v.as_f64().map(TomlValue::Float)
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

/// Write a file with 0o600 permissions so secrets (API keys, tokens) are not
/// world-readable.  On non-Unix platforms falls back to a normal write.
fn write_secret_file(path: &Path, data: &[u8]) -> Result<()> {
    #[cfg(unix)]
    {
        use std::io::Write;
        let mut f = fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .mode(0o600)
            .open(path)?;
        f.write_all(data)?;
        Ok(())
    }
    #[cfg(not(unix))]
    {
        fs::write(path, data)?;
        Ok(())
    }
}

fn supports_config_dir(runtime: &str) -> bool {
    runtime_supports_config_dir(runtime)
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

        let dir = generate_config_dir(&config, "zeroclaw", "abc123", None)
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
    fn template_seeded_config_preserves_defaults_and_overlays_clawden_values() {
        use super::generate_toml_config;

        // Simulate a template config that `onboard` would generate — it
        // contains required fields like default_temperature that clawden
        // doesn't know about.
        let mut template = toml::Table::new();
        template.insert(
            "default_provider".to_string(),
            toml::Value::String("openrouter".to_string()),
        );
        template.insert(
            "default_model".to_string(),
            toml::Value::String("anthropic/claude-sonnet-4.6".to_string()),
        );
        template.insert("default_temperature".to_string(), toml::Value::Float(0.7));
        let mut reliability = toml::Table::new();
        reliability.insert("provider_retries".to_string(), toml::Value::Integer(2));
        reliability.insert("api_keys".to_string(), toml::Value::Array(vec![]));
        template.insert("reliability".to_string(), toml::Value::Table(reliability));
        let mut memory = toml::Table::new();
        memory.insert(
            "backend".to_string(),
            toml::Value::String("sqlite".to_string()),
        );
        template.insert("memory".to_string(), toml::Value::Table(memory));

        // Now build clawden config that should overlay on top.
        let yaml = r#"
runtime: zeroclaw
provider: openrouter
model: anthropic/claude-sonnet-4-6
providers:
  openrouter:
    api_key: sk-or-merged
channels:
  tg:
    type: telegram
    token: tg-merged-token
config:
  debug: true
"#;
        let mut config = ClawDenYaml::parse_yaml(yaml).expect("yaml parse");
        config.resolve_env_vars().expect("resolve env");

        let merged = generate_toml_config(&config, "zeroclaw", Some(&template));

        // Clawden values should override template values.
        assert_eq!(
            merged.get("default_provider").and_then(toml::Value::as_str),
            Some("openrouter")
        );
        assert_eq!(
            merged.get("default_model").and_then(toml::Value::as_str),
            Some("anthropic/claude-sonnet-4-6")
        );

        // Template-only fields like default_temperature MUST survive.
        assert_eq!(
            merged
                .get("default_temperature")
                .and_then(toml::Value::as_float),
            Some(0.7)
        );

        // Template-only nested fields like memory.backend MUST survive.
        assert_eq!(
            merged
                .get("memory")
                .and_then(|v| v.get("backend"))
                .and_then(toml::Value::as_str),
            Some("sqlite")
        );

        // API keys from clawden should be present.
        assert_eq!(
            merged
                .get("reliability")
                .and_then(|v| v.get("api_keys"))
                .and_then(toml::Value::as_array)
                .and_then(|a| a.first())
                .and_then(toml::Value::as_str),
            Some("sk-or-merged")
        );

        // Channel config from clawden should be present.
        assert_eq!(
            merged
                .get("channels_config")
                .and_then(|v| v.get("telegram"))
                .and_then(|v| v.get("bot_token"))
                .and_then(toml::Value::as_str),
            Some("tg-merged-token")
        );

        // Custom config override from clawden should be present.
        assert_eq!(
            merged.get("debug").and_then(toml::Value::as_bool),
            Some(true)
        );
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
        let dir = generate_config_dir(&config, "zeroclaw", "cleanup-ph", None)
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

        let generated_dir = generate_config_dir(&config, "zeroclaw", "iso-ph", None)
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

    #[test]
    fn generates_openfang_toml_and_injects_config_dir() {
        let _guard = test_env_lock().lock().expect("env lock");
        let original_home = std::env::var("HOME").ok();
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let tmp_home = std::env::temp_dir().join(format!("clawden-openfang-config-{unique}"));
        fs::create_dir_all(&tmp_home).expect("tmp home");
        std::env::set_var("HOME", &tmp_home);

        let yaml = r#"
runtime: openfang
provider: openrouter
model: anthropic/claude-sonnet-4-6
providers:
  openrouter:
    api_key: sk-openfang-test
channels:
  ops-tg:
    type: telegram
    token: tg-openfang-token
config:
  dashboard:
    enabled: true
"#;
        let mut config = ClawDenYaml::parse_yaml(yaml).expect("yaml parse");
        config.resolve_env_vars().expect("resolve env");

        let dir = generate_config_dir(&config, "openfang", "openfang-ph", None)
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
                .get("channels_config")
                .and_then(|v| v.get("telegram"))
                .and_then(|v| v.get("bot_token"))
                .and_then(toml::Value::as_str),
            Some("tg-openfang-token")
        );
        assert_eq!(
            parsed
                .get("dashboard")
                .and_then(|v| v.get("enabled"))
                .and_then(toml::Value::as_bool),
            Some(true)
        );

        let mut args = vec!["daemon".to_string()];
        inject_config_dir_arg("openfang", &mut args, &dir);
        assert!(args
            .windows(2)
            .any(|w| { w[0] == "--config-dir" && w[1] == dir.to_string_lossy() }));

        if let Some(home) = original_home {
            std::env::set_var("HOME", home);
        } else {
            std::env::remove_var("HOME");
        }
        let _ = fs::remove_dir_all(tmp_home);
    }

    #[test]
    fn generates_picoclaw_json_with_system_prompt_override() {
        let _guard = test_env_lock().lock().expect("env lock");
        let original_home = std::env::var("HOME").ok();
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let tmp_home = std::env::temp_dir().join(format!("clawden-picoclaw-config-{unique}"));
        fs::create_dir_all(&tmp_home).expect("tmp home");
        std::env::set_var("HOME", &tmp_home);

        let yaml = r#"
runtime: picoclaw
provider: openai
providers:
  openai:
    api_key: sk-pico-test
config:
  system_prompt: You are concise
"#;
        let mut config = ClawDenYaml::parse_yaml(yaml).expect("yaml parse");
        config.resolve_env_vars().expect("resolve env");

        let dir = generate_config_dir(&config, "picoclaw", "picoclaw-ph", None)
            .expect("config dir")
            .expect("supported runtime");
        let body = fs::read_to_string(dir.join("config.json")).expect("read config");
        let parsed: serde_json::Value = serde_json::from_str(&body).expect("valid json");

        assert_eq!(
            parsed
                .get("systemPrompt")
                .and_then(serde_json::Value::as_str),
            Some("You are concise")
        );
        assert!(parsed.get("system_prompt").is_none());

        if let Some(home) = original_home {
            std::env::set_var("HOME", home);
        } else {
            std::env::remove_var("HOME");
        }
        let _ = fs::remove_dir_all(tmp_home);
    }
}
