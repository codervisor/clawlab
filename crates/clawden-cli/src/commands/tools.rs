use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::cli::ToolCommand;

#[derive(Debug, Clone, PartialEq, Eq)]
enum ToolStatus {
    Activated,
    Installed,
    Available,
}

impl ToolStatus {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Activated => "activated",
            Self::Installed => "installed",
            Self::Available => "available",
        }
    }
}

#[derive(Debug, Clone, Default)]
struct ToolManifest {
    name: String,
    description: String,
    tier: String,
    version: Option<String>,
    installed_size: Option<String>,
    download_size: Option<String>,
    requires: Vec<String>,
    conflicts: Vec<String>,
    provides: Vec<String>,
}

#[derive(Debug, Clone)]
struct ToolEntry {
    manifest: ToolManifest,
    status: ToolStatus,
}

pub fn exec_tools(command: ToolCommand) -> Result<()> {
    match command {
        ToolCommand::List { installed } => exec_tools_list(installed),
        ToolCommand::Info { tool } => exec_tools_info(&tool),
    }
}

fn exec_tools_list(only_installed: bool) -> Result<()> {
    let entries = load_tool_entries()?;

    println!("TOOL         TIER       SIZE    STATUS      DESCRIPTION");
    for entry in entries {
        if only_installed && entry.status == ToolStatus::Available {
            continue;
        }

        let size = entry
            .manifest
            .installed_size
            .clone()
            .unwrap_or_else(|| "-".to_string());

        println!(
            "{:<12} {:<10} {:<7} {:<11} {}",
            entry.manifest.name,
            fallback(&entry.manifest.tier),
            size,
            entry.status.as_str(),
            fallback(&entry.manifest.description),
        );
    }

    Ok(())
}

fn exec_tools_info(tool_name: &str) -> Result<()> {
    let entries = load_tool_entries()?;
    let Some(entry) = entries
        .iter()
        .find(|entry| entry.manifest.name == tool_name)
    else {
        anyhow::bail!("unknown tool '{}'. Run 'clawden tools list'", tool_name);
    };

    let manifest = &entry.manifest;
    println!("Tool: {}", manifest.name);
    println!("Description: {}", fallback(&manifest.description));
    println!("Tier: {}", fallback(&manifest.tier));
    println!("Status: {}", entry.status.as_str());
    println!(
        "Version: {}",
        manifest.version.clone().unwrap_or_else(|| "-".to_string())
    );
    println!(
        "Installed Size: {}",
        manifest
            .installed_size
            .clone()
            .unwrap_or_else(|| "-".to_string())
    );
    println!(
        "Download Size: {}",
        manifest
            .download_size
            .clone()
            .unwrap_or_else(|| "-".to_string())
    );
    println!("Requires: {}", join_or_dash(&manifest.requires));
    println!("Conflicts: {}", join_or_dash(&manifest.conflicts));
    println!("Provides: {}", join_or_dash(&manifest.provides));

    Ok(())
}

fn join_or_dash(values: &[String]) -> String {
    if values.is_empty() {
        "-".to_string()
    } else {
        values.join(", ")
    }
}

fn fallback(value: &str) -> String {
    if value.trim().is_empty() {
        "-".to_string()
    } else {
        value.to_string()
    }
}

fn load_tool_entries() -> Result<Vec<ToolEntry>> {
    let activated = read_activated_tools();
    let manifests = discover_manifests()?;

    let mut entries: Vec<ToolEntry> = manifests
        .into_iter()
        .map(|(name, manifest, setup_exists)| {
            let status = if activated.contains(&name) {
                ToolStatus::Activated
            } else if setup_exists {
                ToolStatus::Installed
            } else {
                ToolStatus::Available
            };
            ToolEntry { manifest, status }
        })
        .collect();

    entries.sort_by(|a, b| a.manifest.name.cmp(&b.manifest.name));
    Ok(entries)
}

fn discover_manifests() -> Result<Vec<(String, ToolManifest, bool)>> {
    let mut manifests: HashMap<String, (ToolManifest, bool)> = HashMap::new();

    for root in candidate_tool_roots() {
        if !root.exists() {
            continue;
        }

        let entries =
            fs::read_dir(&root).with_context(|| format!("reading tool root {}", root.display()))?;

        for dir_entry in entries {
            let dir_entry = dir_entry?;
            let path = dir_entry.path();
            if !path.is_dir() {
                continue;
            }

            let manifest_path = path.join("manifest.toml");
            if !manifest_path.exists() {
                continue;
            }

            let content = fs::read_to_string(&manifest_path)
                .with_context(|| format!("reading {}", manifest_path.display()))?;
            let mut manifest = parse_manifest(&content);

            if manifest.name.is_empty() {
                if let Some(name) = path.file_name().and_then(|v| v.to_str()) {
                    manifest.name = name.to_string();
                }
            }

            let setup_exists = path.join("setup.sh").exists();
            manifests.insert(manifest.name.clone(), (manifest, setup_exists));
        }
    }

    let mut rows = manifests
        .into_iter()
        .map(|(name, (manifest, setup_exists))| (name, manifest, setup_exists))
        .collect::<Vec<_>>();
    rows.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(rows)
}

fn candidate_tool_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();

    roots.push(PathBuf::from("/opt/clawden/tools"));

    if let Ok(home) = std::env::var("HOME") {
        roots.push(Path::new(&home).join(".clawden/tools"));
        roots.push(Path::new(&home).join(".clawden/community-tools"));
    }

    roots
}

fn read_activated_tools() -> HashSet<String> {
    let mut candidates = vec![PathBuf::from("/run/clawden/tools.json")];

    if let Ok(home) = std::env::var("HOME") {
        candidates.push(Path::new(&home).join(".clawden/run/tools.json"));
    }

    for candidate in candidates {
        let Ok(content) = fs::read_to_string(&candidate) else {
            continue;
        };
        let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&content) else {
            continue;
        };
        let Some(array) = parsed.get("activated").and_then(|v| v.as_array()) else {
            continue;
        };

        let mut result = HashSet::new();
        for value in array {
            if let Some(name) = value.as_str() {
                result.insert(name.to_string());
            }
        }
        return result;
    }

    HashSet::new()
}

fn parse_manifest(content: &str) -> ToolManifest {
    let mut manifest = ToolManifest::default();
    let mut section = String::new();

    for raw in content.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        if line.starts_with('[') && line.ends_with(']') {
            section = line.trim_matches(|c| c == '[' || c == ']').to_string();
            continue;
        }

        let Some((key, value)) = line.split_once('=') else {
            continue;
        };

        let key = key.trim();
        let value = value.trim();

        match (section.as_str(), key) {
            ("tool", "name") => manifest.name = parse_string(value),
            ("tool", "description") => manifest.description = parse_string(value),
            ("tool", "tier") => manifest.tier = parse_string(value),
            ("tool", "version") => manifest.version = Some(parse_string(value)),
            ("size", "installed") => manifest.installed_size = Some(parse_string(value)),
            ("size", "download") => manifest.download_size = Some(parse_string(value)),
            ("dependencies", "requires") => manifest.requires = parse_array(value),
            ("dependencies", "conflicts") => manifest.conflicts = parse_array(value),
            ("capabilities", "provides") => manifest.provides = parse_array(value),
            _ => {}
        }
    }

    manifest
}

fn parse_string(value: &str) -> String {
    value.trim().trim_matches('"').to_string()
}

fn parse_array(value: &str) -> Vec<String> {
    let inner = value.trim().trim_start_matches('[').trim_end_matches(']');
    if inner.trim().is_empty() {
        return Vec::new();
    }

    inner
        .split(',')
        .map(|item| item.trim().trim_matches('"'))
        .filter(|item| !item.is_empty())
        .map(str::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::parse_manifest;

    #[test]
    fn parse_manifest_extracts_core_fields() {
        let data = r#"
[tool]
name = "sandbox"
description = "Execution sandbox"
tier = "standard"
version = "0.9"

[size]
installed = "5MB"

[dependencies]
requires = ["python"]
conflicts = []

[capabilities]
provides = ["bwrap", "clawden-sandbox"]
"#;

        let manifest = parse_manifest(data);
        assert_eq!(manifest.name, "sandbox");
        assert_eq!(manifest.tier, "standard");
        assert_eq!(manifest.installed_size.as_deref(), Some("5MB"));
        assert_eq!(manifest.requires, vec!["python"]);
        assert_eq!(manifest.provides, vec!["bwrap", "clawden-sandbox"]);
    }
}
