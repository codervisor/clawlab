use anyhow::Result;
use clawden_config::{ChannelCredentialMapper, ClawDenYaml};
use clawden_core::{channel_descriptor, runtime_descriptor, ConfigDirFlag, ConfigFormat};
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::fs;
#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use toml::Value as TomlValue;
use tracing::debug;

use super::up::{channel_credential_value, channels_for_runtime, runtime_provider_and_model};

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

    let Some(descriptor) = runtime_descriptor(runtime) else {
        return Ok(None);
    };

    match descriptor.config_format {
        ConfigFormat::Toml => {
            let base = if descriptor.has_onboard_command {
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
        ConfigFormat::Json => {
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
    runtime_descriptor(runtime)
        .map(|descriptor| descriptor.has_onboard_command)
        .unwrap_or(false)
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

    let Some(descriptor) = runtime_descriptor(runtime) else {
        return;
    };

    match descriptor.config_dir_flag {
        ConfigDirFlag::ConfigDir => {
            args.insert(insert_at, "--config-dir".to_string());
            args.insert(insert_at + 1, config_dir.to_string_lossy().to_string());
        }
        ConfigDirFlag::ConfigFile { filename } => {
            args.insert(insert_at, "--config".to_string());
            args.insert(
                insert_at + 1,
                config_dir.join(filename).to_string_lossy().to_string(),
            );
        }
    }
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
        if let Some(descriptor) = channel_descriptor(&channel_type) {
            for credential in descriptor
                .required_credentials
                .iter()
                .chain(descriptor.optional_credentials.iter())
            {
                if let Some(value) = channel_credential_value(channel, credential) {
                    row.insert((*credential).to_string(), TomlValue::String(value));
                }
            }
            if descriptor.supports_allowed_users {
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
        if !row.is_empty() {
            channels_cfg.insert(channel_type, TomlValue::Table(row));
        }
    }
    if !channels_cfg.is_empty() {
        // Merge channel entries into existing channels_config from the base
        // template, preserving non-channel keys like `cli = true` that the
        // runtime requires.
        if let Some(TomlValue::Table(existing)) = root.get_mut("channels_config") {
            existing.extend(channels_cfg);
        } else {
            root.insert(
                "channels_config".to_string(),
                TomlValue::Table(channels_cfg),
            );
        }
    }

    if let Some(descriptor) = runtime_descriptor(runtime) {
        for (section, key, value) in descriptor.required_config_defaults {
            let section_table = root
                .entry((*section).to_string())
                .or_insert_with(|| TomlValue::Table(toml::Table::new()));
            if let TomlValue::Table(table) = section_table {
                let parsed = value
                    .parse::<bool>()
                    .map(TomlValue::Boolean)
                    .unwrap_or_else(|_| TomlValue::String((*value).to_string()));
                table.entry((*key).to_string()).or_insert(parsed);
            }
        }
    }

    // Auto-enable HTTP proxy in the runtime config when the host environment
    // has proxy env vars set.  Without this, runtimes like zeroclaw whose
    // template config defaults to `[proxy] enabled = false` will ignore the
    // inherited proxy environment and fail to reach external APIs.
    inject_proxy_config(&mut root);

    // Inject a relaxed security profile so the runtime defers resource limits,
    // seccomp, capability dropping, and sandbox to ClawDen's outer layer.
    inject_security_profile(&mut root, runtime);

    merge_json_into_toml(&mut root, runtime_config_overrides(config, runtime));
    root
}

/// Detect HTTP proxy environment variables from the host and populate the
/// `[proxy]` section in the runtime config accordingly.
fn inject_proxy_config(root: &mut toml::Table) {
    let Some(settings) = detect_proxy_settings() else {
        return;
    };
    let mut emitter = TomlProxyEmitter { root };
    inject_proxy_config_with_emitter(&mut emitter, &settings);
}

/// Inject a `[security]` section with a "managed" profile so the runtime
/// defers resource limits, seccomp, capability dropping, and tool sandboxing
/// to ClawDen.  Applies to TOML-based runtimes: ZeroClaw, OpenFang, NullClaw.
fn inject_security_profile(root: &mut toml::Table, runtime: &str) {
    let sec = root
        .entry("security".to_string())
        .or_insert_with(|| TomlValue::Table(toml::Table::new()));
    let TomlValue::Table(table) = sec else {
        return;
    };
    table
        .entry("profile".to_string())
        .or_insert(TomlValue::String("managed".to_string()));
    table
        .entry("rlimit_as".to_string())
        .or_insert(TomlValue::Integer(0));
    table
        .entry("rlimit_nofile".to_string())
        .or_insert(TomlValue::Integer(0));
    table
        .entry("rlimit_nproc".to_string())
        .or_insert(TomlValue::Integer(0));
    table
        .entry("seccomp".to_string())
        .or_insert(TomlValue::String("disabled".to_string()));
    table
        .entry("drop_capabilities".to_string())
        .or_insert(TomlValue::Boolean(false));
    table
        .entry("sandbox_tools".to_string())
        .or_insert(TomlValue::Boolean(false));

    // OpenFang-specific: relax gRPC TLS and bind restrictions so ClawDen can
    // reach the health endpoint and manage the network layer.
    if runtime == "openfang" {
        table
            .entry("tls_required".to_string())
            .or_insert(TomlValue::Boolean(false));
        table
            .entry("bind_address".to_string())
            .or_insert(TomlValue::String("0.0.0.0".to_string()));
    }
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

    inject_proxy_config_json(&mut root);

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

pub(crate) fn generate_openclaw_config(
    config: &ClawDenYaml,
    runtime: &str,
) -> serde_json::Map<String, JsonValue> {
    let mut root = serde_json::Map::new();

    let mut channels = serde_json::Map::new();
    for channel_name in channels_for_runtime(config, runtime) {
        let Some(channel) = config.channels.get(&channel_name) else {
            continue;
        };
        let channel_type = ClawDenYaml::resolve_channel_type(&channel_name, channel)
            .unwrap_or_else(|| channel_name.clone());
        if let Ok(JsonValue::Object(obj)) =
            ChannelCredentialMapper::openclaw_channel_config(&channel_type, channel)
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

    inject_runtime_agent_model(&mut root, config, runtime);

    root
}

fn inject_runtime_agent_model(
    root: &mut serde_json::Map<String, JsonValue>,
    config: &ClawDenYaml,
    runtime: &str,
) {
    let Some(transform) = runtime_descriptor(runtime).and_then(|d| d.model_transform) else {
        return;
    };
    // Skip if the user already set agents.defaults.model via config overrides.
    if root
        .get("agents")
        .and_then(|a| a.get("defaults"))
        .and_then(|d| d.get("model"))
        .is_some()
    {
        return;
    }

    let Some((provider_name, _provider, model_opt)) = runtime_provider_and_model(config, runtime)
    else {
        return;
    };

    let provider_lower = provider_name.to_ascii_lowercase();

    // OpenClaw's built-in default: anthropic/claude-opus-4-6.
    // If the configured provider already matches, no override is needed.
    if provider_lower == "anthropic" {
        return;
    }

    // Use the explicit model if provided, otherwise fall back to OpenClaw's
    // built-in default so we can re-prefix it through the configured provider.
    let model = model_opt.unwrap_or_else(|| "anthropic/claude-opus-4-6".to_string());

    let model_ref = transform(&provider_lower, &model);

    let agents = root
        .entry("agents".to_string())
        .or_insert_with(|| JsonValue::Object(serde_json::Map::new()));
    if let JsonValue::Object(agents_obj) = agents {
        let defaults = agents_obj
            .entry("defaults".to_string())
            .or_insert_with(|| JsonValue::Object(serde_json::Map::new()));
        if let JsonValue::Object(defaults_obj) = defaults {
            defaults_obj.insert("model".to_string(), JsonValue::String(model_ref));
        }
    }
}

pub(crate) fn write_env_runtime_config(
    config: &ClawDenYaml,
    runtime: &str,
    project_hash: &str,
) -> Result<()> {
    let Some(descriptor) = runtime_descriptor(runtime) else {
        return Ok(());
    };
    if descriptor.config_format != ConfigFormat::EnvVars {
        return Ok(());
    }

    let dir = runtime_config_dir(project_hash, runtime)?;
    fs::create_dir_all(&dir)?;

    if runtime == "openclaw" {
        let body = generate_openclaw_config(config, runtime);
        write_secret_file(
            &dir.join("openclaw.json"),
            serde_json::to_string_pretty(&body)?.as_bytes(),
        )?;
    }

    Ok(())
}

/// Detect HTTP proxy environment variables from the host and populate a
/// `"proxy"` object in a JSON runtime config (picoclaw).
fn inject_proxy_config_json(root: &mut serde_json::Map<String, JsonValue>) {
    let Some(settings) = detect_proxy_settings() else {
        return;
    };
    let mut emitter = JsonProxyEmitter { root };
    inject_proxy_config_with_emitter(&mut emitter, &settings);
}

struct ProxySettings {
    https_proxy: Option<String>,
    http_proxy: Option<String>,
    no_proxy: Option<Vec<String>>,
}

trait ProxyConfigEmitter {
    fn set_bool(&mut self, key: &str, value: bool);
    fn set_string(&mut self, key: &str, value: String);
    fn set_string_array(&mut self, key: &str, value: Vec<String>);
}

struct TomlProxyEmitter<'a> {
    root: &'a mut toml::Table,
}

impl ProxyConfigEmitter for TomlProxyEmitter<'_> {
    fn set_bool(&mut self, key: &str, value: bool) {
        let proxy = self
            .root
            .entry("proxy".to_string())
            .or_insert_with(|| TomlValue::Table(toml::Table::new()));
        if let TomlValue::Table(table) = proxy {
            table.insert(key.to_string(), TomlValue::Boolean(value));
        }
    }

    fn set_string(&mut self, key: &str, value: String) {
        let proxy = self
            .root
            .entry("proxy".to_string())
            .or_insert_with(|| TomlValue::Table(toml::Table::new()));
        if let TomlValue::Table(table) = proxy {
            table.insert(key.to_string(), TomlValue::String(value));
        }
    }

    fn set_string_array(&mut self, key: &str, value: Vec<String>) {
        let proxy = self
            .root
            .entry("proxy".to_string())
            .or_insert_with(|| TomlValue::Table(toml::Table::new()));
        if let TomlValue::Table(table) = proxy {
            table.insert(
                key.to_string(),
                TomlValue::Array(value.into_iter().map(TomlValue::String).collect()),
            );
        }
    }
}

struct JsonProxyEmitter<'a> {
    root: &'a mut serde_json::Map<String, JsonValue>,
}

impl ProxyConfigEmitter for JsonProxyEmitter<'_> {
    fn set_bool(&mut self, key: &str, value: bool) {
        let proxy = self
            .root
            .entry("proxy".to_string())
            .or_insert_with(|| JsonValue::Object(serde_json::Map::new()));
        if let JsonValue::Object(table) = proxy {
            table.insert(to_camel_case(key), JsonValue::Bool(value));
        }
    }

    fn set_string(&mut self, key: &str, value: String) {
        let proxy = self
            .root
            .entry("proxy".to_string())
            .or_insert_with(|| JsonValue::Object(serde_json::Map::new()));
        if let JsonValue::Object(table) = proxy {
            table.insert(to_camel_case(key), JsonValue::String(value));
        }
    }

    fn set_string_array(&mut self, key: &str, value: Vec<String>) {
        let proxy = self
            .root
            .entry("proxy".to_string())
            .or_insert_with(|| JsonValue::Object(serde_json::Map::new()));
        if let JsonValue::Object(table) = proxy {
            table.insert(
                to_camel_case(key),
                JsonValue::Array(value.into_iter().map(JsonValue::String).collect()),
            );
        }
    }
}

fn inject_proxy_config_with_emitter(
    emitter: &mut dyn ProxyConfigEmitter,
    settings: &ProxySettings,
) {
    emitter.set_bool("enabled", true);
    // Use "environment" scope so ALL HTTP clients in the runtime process route through the proxy.
    emitter.set_string("scope", "environment".to_string());
    if let Some(url) = &settings.https_proxy {
        emitter.set_string("https_proxy", url.clone());
    }
    if let Some(url) = &settings.http_proxy {
        emitter.set_string("http_proxy", url.clone());
    }
    if let Some(hosts) = &settings.no_proxy {
        emitter.set_string_array("no_proxy", hosts.clone());
    }
}

fn detect_proxy_settings() -> Option<ProxySettings> {
    let https_proxy = std::env::var("https_proxy")
        .or_else(|_| std::env::var("HTTPS_PROXY"))
        .ok()
        .filter(|v| !v.trim().is_empty());
    let http_proxy = std::env::var("http_proxy")
        .or_else(|_| std::env::var("HTTP_PROXY"))
        .ok()
        .filter(|v| !v.trim().is_empty());
    let no_proxy = std::env::var("no_proxy")
        .or_else(|_| std::env::var("NO_PROXY"))
        .ok()
        .filter(|v| !v.trim().is_empty())
        .map(|hosts| hosts.split(',').map(|s| s.trim().to_string()).collect());

    if https_proxy.is_none() && http_proxy.is_none() {
        return None;
    }

    Some(ProxySettings {
        https_proxy,
        http_proxy,
        no_proxy,
    })
}

fn to_camel_case(key: &str) -> String {
    if !key.contains('_') {
        return key.to_string();
    }
    let mut out = String::new();
    let mut upper = false;
    for ch in key.chars() {
        if ch == '_' {
            upper = true;
            continue;
        }
        if upper {
            out.push(ch.to_ascii_uppercase());
            upper = false;
        } else {
            out.push(ch);
        }
    }
    out
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
    runtime_descriptor(runtime)
        .map(|descriptor| descriptor.supports_config_dir)
        .unwrap_or(false)
}

/// Create a project-isolated state directory for runtimes that use env vars
/// (not `--config-dir`) for isolation.  Returns env var pairs to inject.
pub(crate) fn state_dir_env_vars(
    runtime: &str,
    project_hash: &str,
) -> Result<Vec<(String, String)>> {
    let descriptor = match runtime_descriptor(runtime) {
        Some(d) => d,
        None => return Ok(Vec::new()),
    };
    // Only applies to env-var-based runtimes that don't use --config-dir
    if descriptor.supports_config_dir || descriptor.config_format != ConfigFormat::EnvVars {
        return Ok(Vec::new());
    }
    let dir = runtime_config_dir(project_hash, runtime)?;
    fs::create_dir_all(&dir)?;
    let runtime_key = clawden_core::runtime_env_prefix(runtime);
    let dir_str = dir.to_string_lossy().to_string();
    let mut vars = vec![(format!("{runtime_key}_STATE_DIR"), dir_str.clone())];
    for (env_name, _) in descriptor.extra_env_vars {
        if *env_name == "OPENCLAW_CONFIG_PATH" {
            let config_path = dir.join("openclaw.json");
            vars.push((
                (*env_name).to_string(),
                config_path.to_string_lossy().to_string(),
            ));
        }
    }
    Ok(vars)
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
    use super::{
        cleanup_project_config_dir, generate_config_dir, generate_openclaw_config,
        inject_config_dir_arg, write_env_runtime_config,
    };
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
        // zeroclaw requires `cli = true` in channels_config
        assert_eq!(
            parsed
                .get("channels_config")
                .and_then(|v| v.get("cli"))
                .and_then(toml::Value::as_bool),
            Some(true)
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

        // zeroclaw requires `cli = true` in channels_config
        assert_eq!(
            merged
                .get("channels_config")
                .and_then(|v| v.get("cli"))
                .and_then(toml::Value::as_bool),
            Some(true)
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
    fn generates_openclaw_json_with_telegram_allow_from() {
        let yaml = r#"
runtime: openclaw
channels:
  telegram:
    token: tg-test-token
    allowed_users: ["12345", "67890"]
"#;
        let mut config = ClawDenYaml::parse_yaml(yaml).expect("yaml parse");
        config.resolve_env_vars().expect("resolve env");

        let generated = generate_openclaw_config(&config, "openclaw");

        assert_eq!(
            generated
                .get("channels")
                .and_then(|v| v.get("telegram"))
                .and_then(|v| v.get("botToken"))
                .and_then(serde_json::Value::as_str),
            Some("tg-test-token")
        );
        assert_eq!(
            generated
                .get("channels")
                .and_then(|v| v.get("telegram"))
                .and_then(|v| v.get("dmPolicy"))
                .and_then(serde_json::Value::as_str),
            Some("allowlist")
        );
        assert_eq!(
            generated
                .get("channels")
                .and_then(|v| v.get("telegram"))
                .and_then(|v| v.get("allowFrom"))
                .and_then(serde_json::Value::as_array)
                .map(Vec::len),
            Some(2)
        );
    }

    #[test]
    fn writes_openclaw_json_to_project_state_dir() {
        let _guard = test_env_lock().lock().expect("env lock");
        let original_home = std::env::var("HOME").ok();
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let tmp_home = std::env::temp_dir().join(format!("clawden-openclaw-config-{unique}"));
        fs::create_dir_all(&tmp_home).expect("tmp home");
        std::env::set_var("HOME", &tmp_home);

        let yaml = r#"
runtime: openclaw
channels:
  telegram:
    token: tg-test-token
    allowed_users: ["12345"]
"#;
        let mut config = ClawDenYaml::parse_yaml(yaml).expect("yaml parse");
        config.resolve_env_vars().expect("resolve env");

        write_env_runtime_config(&config, "openclaw", "openclaw-ph").expect("write config");

        let path = tmp_home
            .join(".clawden")
            .join("configs")
            .join("openclaw-ph")
            .join("openclaw")
            .join("openclaw.json");
        let body = fs::read_to_string(path).expect("read config");
        let parsed: serde_json::Value = serde_json::from_str(&body).expect("valid json");

        assert_eq!(
            parsed
                .get("channels")
                .and_then(|v| v.get("telegram"))
                .and_then(|v| v.get("allowFrom"))
                .and_then(serde_json::Value::as_array)
                .map(Vec::len),
            Some(1)
        );

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
        let config_file = dir.join("config.toml").to_string_lossy().to_string();
        assert!(args
            .windows(2)
            .any(|w| { w[0] == "--config" && w[1] == config_file }));

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

    #[test]
    fn generates_picoclaw_json_with_proxy_from_env() {
        let _guard = test_env_lock().lock().expect("env lock");
        let original_home = std::env::var("HOME").ok();
        let orig_https = std::env::var("HTTPS_PROXY").ok();
        let orig_http = std::env::var("HTTP_PROXY").ok();
        let orig_no = std::env::var("NO_PROXY").ok();
        // Clear lowercase variants too
        let orig_https_lc = std::env::var("https_proxy").ok();
        let orig_http_lc = std::env::var("http_proxy").ok();
        let orig_no_lc = std::env::var("no_proxy").ok();

        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let tmp_home = std::env::temp_dir().join(format!("clawden-picoclaw-proxy-{unique}"));
        fs::create_dir_all(&tmp_home).expect("tmp home");
        std::env::set_var("HOME", &tmp_home);
        std::env::set_var("HTTPS_PROXY", "http://proxy.corp:3128");
        std::env::set_var("HTTP_PROXY", "http://proxy.corp:3128");
        std::env::set_var("NO_PROXY", "localhost,127.0.0.1,.internal");
        // Remove lowercase so detection uses uppercase
        std::env::remove_var("https_proxy");
        std::env::remove_var("http_proxy");
        std::env::remove_var("no_proxy");

        let yaml = r#"
runtime: picoclaw
provider: openai
providers:
  openai:
    api_key: sk-pico-proxy
channels:
  tg:
    type: telegram
    token: tg-proxy-token
"#;
        let mut config = ClawDenYaml::parse_yaml(yaml).expect("yaml parse");
        config.resolve_env_vars().expect("resolve env");

        let dir = generate_config_dir(&config, "picoclaw", "picoclaw-proxy-ph", None)
            .expect("config dir")
            .expect("supported runtime");
        let body = fs::read_to_string(dir.join("config.json")).expect("read config");
        let parsed: serde_json::Value = serde_json::from_str(&body).expect("valid json");

        let proxy = parsed.get("proxy").expect("proxy section present");
        assert_eq!(proxy.get("enabled").and_then(|v| v.as_bool()), Some(true));
        assert_eq!(
            proxy.get("scope").and_then(|v| v.as_str()),
            Some("environment")
        );
        assert_eq!(
            proxy.get("httpsProxy").and_then(|v| v.as_str()),
            Some("http://proxy.corp:3128")
        );
        assert_eq!(
            proxy.get("httpProxy").and_then(|v| v.as_str()),
            Some("http://proxy.corp:3128")
        );
        let no_proxy = proxy
            .get("noProxy")
            .and_then(|v| v.as_array())
            .expect("noProxy array");
        assert_eq!(no_proxy.len(), 3);
        assert_eq!(no_proxy[0].as_str(), Some("localhost"));
        assert_eq!(no_proxy[1].as_str(), Some("127.0.0.1"));
        assert_eq!(no_proxy[2].as_str(), Some(".internal"));

        // Restore env
        macro_rules! restore_var {
            ($name:expr, $orig:expr) => {
                if let Some(v) = &$orig {
                    std::env::set_var($name, v);
                } else {
                    std::env::remove_var($name);
                }
            };
        }
        restore_var!("HTTPS_PROXY", orig_https);
        restore_var!("HTTP_PROXY", orig_http);
        restore_var!("NO_PROXY", orig_no);
        restore_var!("https_proxy", orig_https_lc);
        restore_var!("http_proxy", orig_http_lc);
        restore_var!("no_proxy", orig_no_lc);
        if let Some(home) = original_home {
            std::env::set_var("HOME", home);
        } else {
            std::env::remove_var("HOME");
        }
        let _ = fs::remove_dir_all(tmp_home);
    }

    #[test]
    fn picoclaw_json_no_proxy_section_when_env_unset() {
        let _guard = test_env_lock().lock().expect("env lock");
        let orig_https = std::env::var("HTTPS_PROXY").ok();
        let orig_http = std::env::var("HTTP_PROXY").ok();
        let orig_https_lc = std::env::var("https_proxy").ok();
        let orig_http_lc = std::env::var("http_proxy").ok();

        std::env::remove_var("HTTPS_PROXY");
        std::env::remove_var("HTTP_PROXY");
        std::env::remove_var("https_proxy");
        std::env::remove_var("http_proxy");

        let yaml = r#"
runtime: picoclaw
provider: openai
providers:
  openai:
    api_key: sk-pico-noproxy
"#;
        let mut config = ClawDenYaml::parse_yaml(yaml).expect("yaml parse");
        config.resolve_env_vars().expect("resolve env");

        let result = super::generate_picoclaw_config(&config, "picoclaw");
        assert!(
            result.get("proxy").is_none(),
            "proxy section should not be present when no proxy env vars set"
        );

        macro_rules! restore_var {
            ($name:expr, $orig:expr) => {
                if let Some(v) = &$orig {
                    std::env::set_var($name, v);
                } else {
                    std::env::remove_var($name);
                }
            };
        }
        restore_var!("HTTPS_PROXY", orig_https);
        restore_var!("HTTP_PROXY", orig_http);
        restore_var!("https_proxy", orig_https_lc);
        restore_var!("http_proxy", orig_http_lc);
    }

    #[test]
    fn openclaw_config_injects_openrouter_prefixed_model() {
        let yaml = r#"
runtime: openclaw
provider: openrouter
model: anthropic/claude-opus-4-6
providers:
  openrouter:
    api_key: sk-or-test
channels:
  telegram:
    token: tg-test-token
"#;
        let mut config = ClawDenYaml::parse_yaml(yaml).expect("yaml parse");
        config.resolve_env_vars().expect("resolve env");

        let generated = generate_openclaw_config(&config, "openclaw");

        assert_eq!(
            generated
                .get("agents")
                .and_then(|v| v.get("defaults"))
                .and_then(|v| v.get("model"))
                .and_then(serde_json::Value::as_str),
            Some("openrouter/anthropic/claude-opus-4-6"),
            "model should be prefixed with the provider for correct routing"
        );
    }

    #[test]
    fn openclaw_config_injects_default_model_for_non_anthropic_provider() {
        let yaml = r#"
runtime: openclaw
provider: openrouter
providers:
  openrouter:
    api_key: sk-or-test
"#;
        let mut config = ClawDenYaml::parse_yaml(yaml).expect("yaml parse");
        config.resolve_env_vars().expect("resolve env");

        let generated = generate_openclaw_config(&config, "openclaw");

        // When no model is specified but provider is not anthropic, the
        // default model should be re-prefixed through the provider.
        assert_eq!(
            generated
                .get("agents")
                .and_then(|v| v.get("defaults"))
                .and_then(|v| v.get("model"))
                .and_then(serde_json::Value::as_str),
            Some("openrouter/anthropic/claude-opus-4-6"),
        );
    }

    #[test]
    fn openclaw_config_skips_model_for_anthropic_provider() {
        let yaml = r#"
runtime: openclaw
provider: anthropic
providers:
  anthropic:
    api_key: sk-ant-test
"#;
        let mut config = ClawDenYaml::parse_yaml(yaml).expect("yaml parse");
        config.resolve_env_vars().expect("resolve env");

        let generated = generate_openclaw_config(&config, "openclaw");

        // When provider matches OpenClaw's default, no model override needed.
        assert!(
            generated
                .get("agents")
                .and_then(|v| v.get("defaults"))
                .and_then(|v| v.get("model"))
                .is_none(),
            "anthropic provider should not inject model override"
        );
    }

    #[test]
    fn openclaw_config_respects_user_model_override() {
        let yaml = r#"
runtime: openclaw
provider: openrouter
model: anthropic/claude-opus-4-6
providers:
  openrouter:
    api_key: sk-or-test
config:
  agents:
    defaults:
      model: "custom/my-model"
"#;
        let mut config = ClawDenYaml::parse_yaml(yaml).expect("yaml parse");
        config.resolve_env_vars().expect("resolve env");

        let generated = generate_openclaw_config(&config, "openclaw");

        // User-provided config override should take precedence.
        assert_eq!(
            generated
                .get("agents")
                .and_then(|v| v.get("defaults"))
                .and_then(|v| v.get("model"))
                .and_then(serde_json::Value::as_str),
            Some("custom/my-model"),
        );
    }
}
