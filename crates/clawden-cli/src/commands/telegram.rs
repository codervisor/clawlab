use anyhow::Result;
use clawden_config::{is_numeric_telegram_id, ChannelInstanceYaml, ClawDenYaml};
use clawden_core::ExecutionMode;
use reqwest::Client;
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::cli::TelegramCommand;

pub async fn exec_telegram(command: TelegramCommand) -> Result<()> {
    match command {
        TelegramCommand::ResolveId { username } => {
            let normalized = normalize_username(&username);
            if normalized.is_empty() {
                anyhow::bail!("username cannot be empty");
            }
            if normalized == "*" {
                anyhow::bail!("wildcard '*' cannot be resolved to a numeric Telegram ID");
            }
            if is_numeric_telegram_id(&normalized) {
                println!("{} -> {}", normalized, normalized);
                return Ok(());
            }

            let token = resolve_default_telegram_token()?;
            let resolver = TelegramIdResolver::new(token)?;
            let id = resolver.resolve_username(&normalized).await?;
            println!("{} -> {}", normalized, id);
            Ok(())
        }
    }
}

pub(crate) async fn resolve_openclaw_telegram_allowed_users_for_runtime(
    config: &mut ClawDenYaml,
    runtime: &str,
) -> Result<()> {
    if runtime != "openclaw" {
        return Ok(());
    }

    let channel_names = super::up::channels_for_runtime(config, runtime);
    for channel_name in channel_names {
        let Some(channel) = config.channels.get_mut(&channel_name) else {
            continue;
        };
        let channel_type = ClawDenYaml::resolve_channel_type(&channel_name, channel)
            .unwrap_or_else(|| channel_name.clone())
            .to_ascii_lowercase();
        if channel_type != "telegram" {
            continue;
        }
        resolve_telegram_allowed_users(channel).await?;
    }

    Ok(())
}

async fn resolve_telegram_allowed_users(channel: &mut ChannelInstanceYaml) -> Result<()> {
    if channel.allowed_users.is_empty() {
        return Ok(());
    }

    let needs_resolution = channel.allowed_users.iter().any(|entry| {
        !entry.trim().is_empty() && entry.trim() != "*" && !is_numeric_telegram_id(entry)
    });
    if !needs_resolution {
        return Ok(());
    }

    let token = channel
        .token
        .as_ref()
        .or(channel.bot_token.as_ref())
        .filter(|v| !v.trim().is_empty())
        .cloned()
        .or_else(|| std::env::var("TELEGRAM_BOT_TOKEN").ok())
        .ok_or_else(|| {
            anyhow::anyhow!(
                "cannot resolve Telegram usernames in allowed_users without TELEGRAM_BOT_TOKEN"
            )
        })?;

    let resolver = TelegramIdResolver::new(token)?;
    let mut resolved = Vec::with_capacity(channel.allowed_users.len());

    for entry in &channel.allowed_users {
        let trimmed = entry.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed == "*" || is_numeric_telegram_id(trimmed) {
            resolved.push(trimmed.to_string());
            continue;
        }

        let username = normalize_username(trimmed);
        let id = resolver.resolve_username(&username).await?;
        eprintln!("Telegram: resolved @{} -> {}", username, id);
        resolved.push(id);
    }

    channel.allowed_users = resolved;
    Ok(())
}

fn resolve_default_telegram_token() -> Result<String> {
    if let Ok(token) = std::env::var("TELEGRAM_BOT_TOKEN") {
        if !token.trim().is_empty() {
            return Ok(token);
        }
    }

    let maybe_cfg = super::up::load_config_with_env_file(None)?;
    let Some(cfg) = maybe_cfg else {
        anyhow::bail!(
            "TELEGRAM_BOT_TOKEN is not set and no clawden.yaml was found to discover a telegram token"
        );
    };

    for (name, channel) in cfg.channels {
        let channel_type = ClawDenYaml::resolve_channel_type(&name, &channel)
            .unwrap_or_else(|| name.clone())
            .to_ascii_lowercase();
        if channel_type != "telegram" {
            continue;
        }
        if let Some(token) = channel
            .token
            .as_ref()
            .or(channel.bot_token.as_ref())
            .filter(|v| !v.trim().is_empty())
        {
            return Ok(token.clone());
        }
    }

    anyhow::bail!(
        "could not find a telegram bot token; set TELEGRAM_BOT_TOKEN or configure channels.<name>.token in clawden.yaml"
    )
}

struct TelegramIdResolver {
    token: String,
    cache_path: PathBuf,
    client: Client,
}

impl TelegramIdResolver {
    fn new(token: String) -> Result<Self> {
        let cache_path = telegram_cache_path(&token)?;
        let client = Client::builder()
            .timeout(Duration::from_secs(8))
            .build()
            .map_err(|e| anyhow::anyhow!("failed to build telegram HTTP client: {e}"))?;
        Ok(Self {
            token,
            cache_path,
            client,
        })
    }

    async fn resolve_username(&self, username: &str) -> Result<String> {
        let username_norm = normalize_username(username);
        if username_norm.is_empty() {
            anyhow::bail!("username cannot be empty");
        }

        let mut cache = self.read_cache();
        if let Some(entry) = cache.get(&username_norm) {
            return Ok(entry.id.clone());
        }

        if let Some(found) = self.find_in_recent_updates(&username_norm).await? {
            self.upsert_cache(&mut cache, &username_norm, &found, false)?;
            return Ok(found);
        }

        eprintln!(
            "⚠ Cannot resolve Telegram username \"{}\" to numeric ID.\n  -> Send any message to your bot from @{}; polling Telegram updates now...",
            username_norm, username_norm
        );

        let timeout_secs = telegram_username_resolution_timeout_secs();
        let deadline = std::time::Instant::now() + Duration::from_secs(timeout_secs);
        while std::time::Instant::now() < deadline {
            if let Some(found) = self.find_in_recent_updates(&username_norm).await? {
                self.upsert_cache(&mut cache, &username_norm, &found, false)?;
                return Ok(found);
            }
            tokio::time::sleep(Duration::from_secs(2)).await;
        }

        let mut message = format!(
            "timed out resolving @{} after {}s. Send a message to your bot from that account and retry.",
            username_norm, timeout_secs
        );
        if self.get_updates_raw().await?.is_empty() && is_openclaw_running() {
            message.push_str(" Another process appears to be polling Telegram updates (OpenClaw running). Stop it first and retry.");
        }
        anyhow::bail!(message)
    }

    async fn find_in_recent_updates(&self, username: &str) -> Result<Option<String>> {
        let updates = self.get_updates_raw().await?;
        for (candidate_user, candidate_id) in collect_username_id_pairs(&updates) {
            if candidate_user == username {
                return Ok(Some(candidate_id));
            }
        }
        Ok(None)
    }

    async fn get_updates_raw(&self) -> Result<Vec<Value>> {
        let url = format!("https://api.telegram.org/bot{}/getUpdates", self.token);
        let resp = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("telegram getUpdates request failed: {e}"))?;
        let payload: Value = resp
            .json()
            .await
            .map_err(|e| anyhow::anyhow!("invalid getUpdates response: {e}"))?;

        let ok = payload.get("ok").and_then(Value::as_bool).unwrap_or(false);
        if !ok {
            let desc = payload
                .get("description")
                .and_then(Value::as_str)
                .unwrap_or("unknown telegram API error");
            anyhow::bail!("telegram getUpdates failed: {desc}");
        }

        Ok(payload
            .get("result")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default())
    }

    fn upsert_cache(
        &self,
        cache: &mut HashMap<String, CacheEntry>,
        username: &str,
        id: &str,
        quiet: bool,
    ) -> Result<()> {
        if let Some(prev) = cache.get(username) {
            if prev.id != id && !quiet {
                eprintln!(
                    "Telegram: @{} was previously mapped to {} but now resolves to {}; cache updated.",
                    username, prev.id, id
                );
            }
        }
        cache.insert(
            username.to_string(),
            CacheEntry {
                id: id.to_string(),
                resolved_at: now_rfc3339_like(),
            },
        );
        self.write_cache(cache)
    }

    fn read_cache(&self) -> HashMap<String, CacheEntry> {
        let Ok(raw) = fs::read_to_string(&self.cache_path) else {
            return HashMap::new();
        };
        let Ok(value) = serde_json::from_str::<Value>(&raw) else {
            return HashMap::new();
        };

        let mut out = HashMap::new();
        let Some(obj) = value.as_object() else {
            return out;
        };

        for (user, v) in obj {
            let Some(entry) = v.as_object() else {
                continue;
            };
            let Some(id_val) = entry.get("id") else {
                continue;
            };
            let id_string = if let Some(id_num) = id_val.as_i64() {
                id_num.to_string()
            } else if let Some(id_str) = id_val.as_str() {
                id_str.to_string()
            } else {
                continue;
            };
            let resolved_at = entry
                .get("resolved_at")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            out.insert(
                normalize_username(user),
                CacheEntry {
                    id: id_string,
                    resolved_at,
                },
            );
        }

        out
    }

    fn write_cache(&self, cache: &HashMap<String, CacheEntry>) -> Result<()> {
        if let Some(parent) = self.cache_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let mut root = Map::new();
        let mut keys = cache.keys().cloned().collect::<Vec<_>>();
        keys.sort();
        for key in keys {
            let entry = cache.get(&key).expect("cache key exists");
            root.insert(
                key,
                serde_json::json!({
                    "id": entry.id,
                    "resolved_at": entry.resolved_at,
                }),
            );
        }

        fs::write(
            &self.cache_path,
            serde_json::to_string_pretty(&Value::Object(root))?,
        )?;
        Ok(())
    }
}

#[derive(Clone)]
struct CacheEntry {
    id: String,
    resolved_at: String,
}

fn normalize_username(input: &str) -> String {
    input.trim().trim_start_matches('@').to_ascii_lowercase()
}

fn collect_username_id_pairs(updates: &[Value]) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for update in updates {
        collect_pairs_from_value(update, &mut out);
    }
    out
}

fn collect_pairs_from_value(value: &Value, out: &mut Vec<(String, String)>) {
    match value {
        Value::Object(map) => {
            if let (Some(username), Some(id)) = (
                map.get("username").and_then(Value::as_str),
                map.get("id")
                    .and_then(Value::as_i64)
                    .map(|n| n.to_string())
                    .or_else(|| map.get("id").and_then(Value::as_str).map(|s| s.to_string())),
            ) {
                out.push((normalize_username(username), id));
            }
            for child in map.values() {
                collect_pairs_from_value(child, out);
            }
        }
        Value::Array(items) => {
            for item in items {
                collect_pairs_from_value(item, out);
            }
        }
        _ => {}
    }
}

fn telegram_cache_path(token: &str) -> Result<PathBuf> {
    let cwd = std::env::current_dir()?;
    let hash = token_hash_prefix(token);
    Ok(cwd
        .join(".clawden")
        .join(format!("telegram-ids-{}.json", hash)))
}

fn token_hash_prefix(token: &str) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    token.hash(&mut hasher);
    format!("{:016x}", hasher.finish())[..8].to_string()
}

fn now_rfc3339_like() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("{}Z", secs)
}

fn telegram_username_resolution_timeout_secs() -> u64 {
    const DEFAULT_SECS: u64 = 120;
    const MIN_SECS: u64 = 10;
    const MAX_SECS: u64 = 600;

    std::env::var("CLAWDEN_TELEGRAM_RESOLVE_TIMEOUT_SECS")
        .ok()
        .and_then(|raw| raw.trim().parse::<u64>().ok())
        .map(|secs| secs.clamp(MIN_SECS, MAX_SECS))
        .unwrap_or(DEFAULT_SECS)
}

fn is_openclaw_running() -> bool {
    let Ok(pm) = clawden_core::ProcessManager::new(ExecutionMode::Auto) else {
        return false;
    };
    pm.list_statuses()
        .map(|statuses| {
            statuses
                .into_iter()
                .any(|s| s.runtime == "openclaw" && s.running)
        })
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::{
        collect_username_id_pairs, normalize_username,
        resolve_openclaw_telegram_allowed_users_for_runtime, telegram_cache_path,
        telegram_username_resolution_timeout_secs, token_hash_prefix,
    };
    use crate::commands::config_gen::generate_openclaw_config;
    use crate::commands::test_env_lock;
    use clawden_config::ClawDenYaml;
    use serde_json::json;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn normalize_username_strips_prefix_and_lowercases() {
        assert_eq!(normalize_username(" @MarvZhang "), "marvzhang");
    }

    #[test]
    fn token_hash_prefix_is_stable() {
        assert_eq!(token_hash_prefix("abc"), token_hash_prefix("abc"));
    }

    #[test]
    fn collect_username_id_pairs_extracts_nested_fields() {
        let updates = vec![json!({
            "message": {
                "from": {"id": 123456, "username": "MarvZhang"}
            }
        })];
        let pairs = collect_username_id_pairs(&updates);
        assert_eq!(pairs, vec![("marvzhang".to_string(), "123456".to_string())]);
    }

    #[test]
    fn telegram_resolution_timeout_defaults_and_clamps() {
        let _guard = test_env_lock().lock().expect("env lock");
        std::env::remove_var("CLAWDEN_TELEGRAM_RESOLVE_TIMEOUT_SECS");
        assert_eq!(telegram_username_resolution_timeout_secs(), 120);

        std::env::set_var("CLAWDEN_TELEGRAM_RESOLVE_TIMEOUT_SECS", "5");
        assert_eq!(telegram_username_resolution_timeout_secs(), 10);

        std::env::set_var("CLAWDEN_TELEGRAM_RESOLVE_TIMEOUT_SECS", "9999");
        assert_eq!(telegram_username_resolution_timeout_secs(), 600);

        std::env::set_var("CLAWDEN_TELEGRAM_RESOLVE_TIMEOUT_SECS", "180");
        assert_eq!(telegram_username_resolution_timeout_secs(), 180);

        std::env::set_var("CLAWDEN_TELEGRAM_RESOLVE_TIMEOUT_SECS", "invalid");
        assert_eq!(telegram_username_resolution_timeout_secs(), 120);

        std::env::remove_var("CLAWDEN_TELEGRAM_RESOLVE_TIMEOUT_SECS");
    }

    #[test]
    fn resolves_cached_username_for_openclaw_telegram_channel() {
        let _guard = test_env_lock().lock().expect("env lock");
        let original_cwd = std::env::current_dir().expect("cwd");
        let original_token = std::env::var("TELEGRAM_BOT_TOKEN").ok();

        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let tmp = std::env::temp_dir().join(format!("clawden-tg-resolver-{stamp}"));
        fs::create_dir_all(&tmp).expect("tmp dir");
        std::env::set_current_dir(&tmp).expect("set cwd");
        std::env::set_var("TELEGRAM_BOT_TOKEN", "bot-token-abc");

        let cache_path = telegram_cache_path("bot-token-abc").expect("cache path");
        fs::create_dir_all(cache_path.parent().expect("cache parent")).expect("cache dir");
        fs::write(
            cache_path,
            r#"{
  "marvzhang": { "id": "123456789", "resolved_at": "2026-03-05T10:00:00Z" }
}"#,
        )
        .expect("cache file");

        let mut cfg = ClawDenYaml::parse_yaml(
            r#"
runtime: openclaw
channels:
  telegram:
    type: telegram
    token: bot-token-abc
    allowed_users: ["marvzhang"]
"#,
        )
        .expect("yaml parse");
        cfg.resolve_env_vars().expect("resolve env");

        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");
        runtime
            .block_on(resolve_openclaw_telegram_allowed_users_for_runtime(
                &mut cfg, "openclaw",
            ))
            .expect("resolution");
        let generated = generate_openclaw_config(&cfg, "openclaw");
        assert_eq!(
            generated
                .get("channels")
                .and_then(|v| v.get("telegram"))
                .and_then(|v| v.get("allowFrom"))
                .and_then(|v| v.as_array())
                .and_then(|arr| arr.first())
                .and_then(|v| v.as_str()),
            Some("123456789")
        );

        if let Some(token) = original_token {
            std::env::set_var("TELEGRAM_BOT_TOKEN", token);
        } else {
            std::env::remove_var("TELEGRAM_BOT_TOKEN");
        }
        std::env::set_current_dir(original_cwd).expect("restore cwd");
        let _ = fs::remove_dir_all(tmp);
    }
}
