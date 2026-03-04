use anyhow::Result;
use clawden_config::SecretVault;
use clawden_core::{version_satisfies, ClawRuntime, InstalledRuntime, RuntimeInstaller};
use std::collections::hash_map::DefaultHasher;
use std::fs::OpenOptions;
use std::hash::{Hash, Hasher};
use std::io::{self, IsTerminal, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

pub fn parse_runtime(value: &str) -> Result<ClawRuntime> {
    ClawRuntime::from_str_loose(value).ok_or_else(|| anyhow::anyhow!("unknown runtime: {value}"))
}

pub fn parse_runtime_version(spec: &str) -> (String, Option<String>) {
    if let Some((runtime, version)) = spec.split_once('@') {
        (runtime.to_string(), Some(version.to_string()))
    } else {
        (spec.to_string(), None)
    }
}

pub fn command_exists(command: &str) -> bool {
    Command::new("which")
        .arg(command)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

pub fn env_no_docker_enabled() -> bool {
    std::env::var("CLAWDEN_NO_DOCKER")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

pub fn ensure_installed_runtime(
    installer: &RuntimeInstaller,
    runtime: &str,
    pinned_version: Option<&str>,
) -> Result<InstalledRuntime> {
    if let Some(exe) = installer.runtime_executable(runtime) {
        if let Some(pin) = pinned_version {
            if let Some(installed_version) = installer.installed_version(runtime)? {
                if !version_satisfies(&installed_version, pin) {
                    println!(
                        "Runtime '{runtime}' installed at {installed_version} but clawden.yaml requires {pin}. Installing compatible version..."
                    );
                    let installed = installer.install_runtime(runtime, Some(pin))?;
                    println!("Installed {}@{}", installed.runtime, installed.version);
                    return Ok(installed);
                }
            }
        }

        return Ok(InstalledRuntime {
            runtime: runtime.to_string(),
            version: "current".to_string(),
            executable: exe,
        });
    }
    let requested = pinned_version.unwrap_or("latest");
    println!("Runtime '{runtime}' not installed. Installing {requested}...");
    let installed = installer.install_runtime(runtime, Some(requested))?;
    println!("Installed {}@{}", installed.runtime, installed.version);
    Ok(installed)
}

pub fn project_hash() -> Result<String> {
    let cwd = std::env::current_dir()?;
    let config_path = cwd.join("clawden.yaml");
    let root = if config_path.exists() {
        std::fs::canonicalize(config_path)?
    } else {
        std::fs::canonicalize(cwd)?
    };

    let mut hasher = DefaultHasher::new();
    root.to_string_lossy().hash(&mut hasher);
    Ok(format!("{:016x}", hasher.finish()))
}

pub fn append_audit_file(action: &str, runtime: &str, outcome: &str) -> Result<()> {
    let home = std::env::var("HOME")?;
    let log_dir = PathBuf::from(home).join(".clawden").join("logs");
    std::fs::create_dir_all(&log_dir)?;
    let log_path = log_dir.join("audit.log");
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before UNIX_EPOCH")
        .as_millis();
    let line = format!("{now}\t{action}\t{runtime}\t{outcome}\n");

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)?;
    file.write_all(line.as_bytes())?;
    Ok(())
}

pub fn is_first_run_context(installer: &RuntimeInstaller) -> Result<bool> {
    let home = std::env::var("HOME")?;
    let clawden_home_exists = PathBuf::from(home).join(".clawden").exists();
    let cwd_has_yaml = std::env::current_dir()?.join("clawden.yaml").exists();
    let has_installed_runtimes = !installer.list_installed()?.is_empty();
    Ok(!clawden_home_exists && !cwd_has_yaml && !has_installed_runtimes)
}

pub fn prompt_yes_no(question: &str, default_yes: bool) -> Result<bool> {
    if !io::stdin().is_terminal() {
        return Ok(false);
    }
    Ok(dialoguer::Confirm::new()
        .with_prompt(question)
        .default(default_yes)
        .interact()?)
}

pub fn store_provider_key_in_vault(provider: &str, key: &str) -> Result<PathBuf> {
    let path = vault_file_path()?;
    let mut vault = load_vault()?;
    vault.put(&provider_secret_name(provider), key);
    save_vault(&vault, &path)?;
    Ok(path)
}

pub fn get_provider_key_from_vault(provider: &str) -> Result<Option<String>> {
    let vault = load_vault()?;
    Ok(vault.get(&provider_secret_name(provider)))
}

fn provider_secret_name(provider: &str) -> String {
    format!("provider/{}", provider.to_ascii_lowercase())
}

fn vault_key() -> Vec<u8> {
    std::env::var("CLAWDEN_VAULT_KEY")
        .unwrap_or_else(|_| "clawden-local-vault-key".to_string())
        .into_bytes()
}

fn vault_file_path() -> Result<PathBuf> {
    let home = std::env::var("HOME")?;
    Ok(PathBuf::from(home).join(".clawden").join("secrets.vault"))
}

fn load_vault() -> Result<SecretVault> {
    let path = vault_file_path()?;
    let key = vault_key();
    if !path.exists() {
        return Ok(SecretVault::new(&key));
    }

    let mut data = std::collections::HashMap::new();
    let content = std::fs::read_to_string(path)?;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if let Some((name, encrypted)) = trimmed.split_once('=') {
            data.insert(name.trim().to_string(), encrypted.trim().to_string());
        }
    }

    SecretVault::from_encrypted_hex(&key, &data).map_err(anyhow::Error::msg)
}

fn save_vault(vault: &SecretVault, path: &PathBuf) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut lines = vec!["# ClawDen encrypted provider key vault".to_string()];
    let mut entries: Vec<_> = vault.export_encrypted_hex().into_iter().collect();
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    for (name, encrypted) in entries {
        lines.push(format!("{name}={encrypted}"));
    }
    std::fs::write(path, format!("{}\n", lines.join("\n")))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    // no unit tests in this module currently
}
