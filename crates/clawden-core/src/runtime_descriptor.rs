use crate::ClawRuntime;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstallSource {
    GithubRelease {
        owner: &'static str,
        repo: &'static str,
        archive_ext: &'static str,
    },
    Npm {
        package: &'static str,
    },
    GitClone {
        url: &'static str,
    },
    NotAvailable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VersionSource {
    GithubLatest {
        owner: &'static str,
        repo: &'static str,
    },
    Npm {
        package: &'static str,
    },
    GitHead {
        url: &'static str,
    },
    NotAvailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigFormat {
    Toml,
    Json,
    EnvVars,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigDirFlag {
    ConfigDir,
    ConfigFile { filename: &'static str },
}

#[derive(Debug, Clone)]
pub struct RuntimeDescriptor {
    pub runtime: ClawRuntime,
    pub slug: &'static str,
    pub display_name: &'static str,
    pub aliases: &'static [&'static str],
    pub install_source: InstallSource,
    pub version_source: VersionSource,
    pub direct_install_supported: bool,
    pub default_start_args: &'static [&'static str],
    pub subcommand_hints: &'static [(&'static str, &'static str)],
    pub config_format: ConfigFormat,
    pub supports_config_dir: bool,
    pub config_dir_flag: ConfigDirFlag,
    pub has_onboard_command: bool,
    pub health_port: Option<u16>,
    pub cost_tier: u8,
    pub required_config_defaults: &'static [(&'static str, &'static str, &'static str)],
    pub extra_env_vars: &'static [(&'static str, &'static str)],
    pub model_transform: Option<fn(provider: &str, model: &str) -> String>,
}

impl RuntimeDescriptor {
    pub fn health_url(&self) -> Option<String> {
        self.health_port
            .map(|port| format!("http://127.0.0.1:{port}/health"))
    }
}

const ZEROCLAW_HINTS: &[(&str, &str)] = &[
    ("daemon", "run as background daemon"),
    ("repl", "interactive REPL"),
    ("chat", "single-turn chat"),
    ("serve", "HTTP API server"),
];

const PICOCLAW_HINTS: &[(&str, &str)] = &[
    ("gateway", "HTTP gateway mode"),
    ("proxy", "reverse proxy mode"),
];

const OPENFANG_HINTS: &[(&str, &str)] = &[
    ("start", "start the daemon"),
    ("chat", "quick chat with default agent"),
];

const OPENCLAW_HINTS: &[(&str, &str)] = &[
    ("gateway", "run the WebSocket gateway (foreground)"),
    ("setup", "initialize local config and workspace"),
    ("tui", "open terminal UI connected to the gateway"),
    ("node run", "run the headless node host (foreground)"),
];

const NULLCLAW_HINTS: &[(&str, &str)] = &[("daemon", "run as background daemon")];

fn openclaw_model_transform(provider: &str, model: &str) -> String {
    let provider_lower = provider.to_ascii_lowercase();
    if model.starts_with(&format!("{provider_lower}/")) {
        model.to_string()
    } else {
        format!("{provider_lower}/{model}")
    }
}

static DESCRIPTORS: &[RuntimeDescriptor] = &[
    RuntimeDescriptor {
        runtime: ClawRuntime::OpenClaw,
        slug: "openclaw",
        display_name: "OpenClaw",
        aliases: &["open-claw", "open"],
        install_source: InstallSource::Npm {
            package: "openclaw",
        },
        version_source: VersionSource::Npm {
            package: "openclaw",
        },
        direct_install_supported: true,
        default_start_args: &["gateway", "--allow-unconfigured"],
        subcommand_hints: OPENCLAW_HINTS,
        config_format: ConfigFormat::EnvVars,
        supports_config_dir: false,
        config_dir_flag: ConfigDirFlag::ConfigDir,
        has_onboard_command: false,
        health_port: Some(18789),
        cost_tier: 3,
        required_config_defaults: &[],
        extra_env_vars: &[("OPENCLAW_CONFIG_PATH", "Path to OpenClaw config file")],
        model_transform: Some(openclaw_model_transform),
    },
    RuntimeDescriptor {
        runtime: ClawRuntime::ZeroClaw,
        slug: "zeroclaw",
        display_name: "ZeroClaw",
        aliases: &["zero-claw", "zero"],
        install_source: InstallSource::GithubRelease {
            owner: "zeroclaw-labs",
            repo: "zeroclaw",
            archive_ext: ".tar.gz",
        },
        version_source: VersionSource::GithubLatest {
            owner: "zeroclaw-labs",
            repo: "zeroclaw",
        },
        direct_install_supported: true,
        default_start_args: &["daemon"],
        subcommand_hints: ZEROCLAW_HINTS,
        config_format: ConfigFormat::Toml,
        supports_config_dir: true,
        config_dir_flag: ConfigDirFlag::ConfigDir,
        has_onboard_command: true,
        health_port: Some(42617),
        cost_tier: 2,
        required_config_defaults: &[("channels_config", "cli", "true")],
        extra_env_vars: &[],
        model_transform: None,
    },
    RuntimeDescriptor {
        runtime: ClawRuntime::PicoClaw,
        slug: "picoclaw",
        display_name: "PicoClaw",
        aliases: &["pico-claw", "pico"],
        install_source: InstallSource::GithubRelease {
            owner: "picoclaw-labs",
            repo: "picoclaw",
            archive_ext: ".7z",
        },
        version_source: VersionSource::GithubLatest {
            owner: "picoclaw-labs",
            repo: "picoclaw",
        },
        direct_install_supported: true,
        default_start_args: &["gateway"],
        subcommand_hints: PICOCLAW_HINTS,
        config_format: ConfigFormat::Json,
        supports_config_dir: true,
        config_dir_flag: ConfigDirFlag::ConfigDir,
        has_onboard_command: false,
        health_port: Some(8080),
        cost_tier: 1,
        required_config_defaults: &[],
        extra_env_vars: &[],
        model_transform: None,
    },
    RuntimeDescriptor {
        runtime: ClawRuntime::NanoClaw,
        slug: "nanoclaw",
        display_name: "NanoClaw",
        aliases: &["nano-claw", "nano"],
        install_source: InstallSource::GitClone {
            url: "https://github.com/qwibitai/nanoclaw.git",
        },
        version_source: VersionSource::GitHead {
            url: "https://github.com/qwibitai/nanoclaw.git",
        },
        direct_install_supported: true,
        default_start_args: &[],
        subcommand_hints: &[],
        config_format: ConfigFormat::EnvVars,
        supports_config_dir: false,
        config_dir_flag: ConfigDirFlag::ConfigDir,
        has_onboard_command: false,
        health_port: None,
        cost_tier: 2,
        required_config_defaults: &[],
        extra_env_vars: &[],
        model_transform: None,
    },
    RuntimeDescriptor {
        runtime: ClawRuntime::IronClaw,
        slug: "ironclaw",
        display_name: "IronClaw",
        aliases: &["iron-claw", "iron"],
        install_source: InstallSource::NotAvailable,
        version_source: VersionSource::NotAvailable,
        direct_install_supported: false,
        default_start_args: &[],
        subcommand_hints: &[],
        config_format: ConfigFormat::None,
        supports_config_dir: false,
        config_dir_flag: ConfigDirFlag::ConfigDir,
        has_onboard_command: false,
        health_port: None,
        cost_tier: 3,
        required_config_defaults: &[],
        extra_env_vars: &[],
        model_transform: None,
    },
    RuntimeDescriptor {
        runtime: ClawRuntime::NullClaw,
        slug: "nullclaw",
        display_name: "NullClaw",
        aliases: &["null-claw", "null"],
        install_source: InstallSource::NotAvailable,
        version_source: VersionSource::NotAvailable,
        direct_install_supported: false,
        default_start_args: &["daemon"],
        subcommand_hints: NULLCLAW_HINTS,
        config_format: ConfigFormat::Toml,
        supports_config_dir: true,
        config_dir_flag: ConfigDirFlag::ConfigDir,
        has_onboard_command: false,
        health_port: Some(3000),
        cost_tier: 1,
        required_config_defaults: &[],
        extra_env_vars: &[],
        model_transform: None,
    },
    RuntimeDescriptor {
        runtime: ClawRuntime::MicroClaw,
        slug: "microclaw",
        display_name: "MicroClaw",
        aliases: &["micro-claw", "micro"],
        install_source: InstallSource::NotAvailable,
        version_source: VersionSource::NotAvailable,
        direct_install_supported: false,
        default_start_args: &[],
        subcommand_hints: &[],
        config_format: ConfigFormat::None,
        supports_config_dir: false,
        config_dir_flag: ConfigDirFlag::ConfigDir,
        has_onboard_command: false,
        health_port: None,
        cost_tier: 1,
        required_config_defaults: &[],
        extra_env_vars: &[],
        model_transform: None,
    },
    RuntimeDescriptor {
        runtime: ClawRuntime::MimiClaw,
        slug: "mimiclaw",
        display_name: "MimiClaw",
        aliases: &["mimi-claw", "mimi"],
        install_source: InstallSource::NotAvailable,
        version_source: VersionSource::NotAvailable,
        direct_install_supported: false,
        default_start_args: &[],
        subcommand_hints: &[],
        config_format: ConfigFormat::None,
        supports_config_dir: false,
        config_dir_flag: ConfigDirFlag::ConfigDir,
        has_onboard_command: false,
        health_port: None,
        cost_tier: 2,
        required_config_defaults: &[],
        extra_env_vars: &[],
        model_transform: None,
    },
    RuntimeDescriptor {
        runtime: ClawRuntime::OpenFang,
        slug: "openfang",
        display_name: "OpenFang",
        aliases: &["open-fang", "fang"],
        install_source: InstallSource::GithubRelease {
            owner: "RightNow-AI",
            repo: "openfang",
            archive_ext: ".tar.gz",
        },
        version_source: VersionSource::GithubLatest {
            owner: "RightNow-AI",
            repo: "openfang",
        },
        direct_install_supported: true,
        default_start_args: &["start"],
        subcommand_hints: OPENFANG_HINTS,
        config_format: ConfigFormat::Toml,
        supports_config_dir: true,
        config_dir_flag: ConfigDirFlag::ConfigFile {
            filename: "config.toml",
        },
        has_onboard_command: false,
        health_port: Some(50051),
        cost_tier: 2,
        required_config_defaults: &[],
        extra_env_vars: &[],
        model_transform: None,
    },
];

pub fn runtime_descriptors() -> &'static [RuntimeDescriptor] {
    DESCRIPTORS
}

pub fn runtime_descriptor(runtime: &str) -> Option<&'static RuntimeDescriptor> {
    let lower = runtime.to_ascii_lowercase();
    DESCRIPTORS
        .iter()
        .find(|d| d.slug == lower || d.aliases.iter().any(|a| *a == lower))
}

pub fn runtime_descriptor_for(runtime: &ClawRuntime) -> Option<&'static RuntimeDescriptor> {
    DESCRIPTORS
        .iter()
        .find(|descriptor| descriptor.runtime == *runtime)
}

pub fn direct_install_descriptors() -> impl Iterator<Item = &'static RuntimeDescriptor> {
    DESCRIPTORS
        .iter()
        .filter(|descriptor| descriptor.direct_install_supported)
}

// --- ClawRuntime impls driven by descriptor data ---

impl std::fmt::Display for ClawRuntime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match runtime_descriptor_for(self) {
            Some(d) => f.write_str(d.display_name),
            None => write!(f, "{self:?}"),
        }
    }
}

impl ClawRuntime {
    pub fn from_str_loose(s: &str) -> Option<Self> {
        runtime_descriptor(s).map(|d| d.runtime.clone())
    }

    pub fn as_slug(&self) -> &'static str {
        runtime_descriptor_for(self)
            .map(|d| d.slug)
            .unwrap_or("unknown")
    }
}

#[cfg(test)]
mod tests {
    use super::{
        direct_install_descriptors, runtime_descriptor, runtime_descriptors, ConfigDirFlag,
    };
    use crate::ClawRuntime;

    #[test]
    fn descriptor_lookup_accepts_aliases() {
        let zero = runtime_descriptor("zero").expect("zero alias should resolve");
        assert_eq!(zero.slug, "zeroclaw");
    }

    #[test]
    fn direct_install_runtime_set_is_descriptor_driven() {
        let slugs: Vec<_> = direct_install_descriptors()
            .map(|descriptor| descriptor.slug)
            .collect();
        assert_eq!(
            slugs,
            vec!["openclaw", "zeroclaw", "picoclaw", "nanoclaw", "openfang"]
        );
    }

    #[test]
    fn display_and_slug_roundtrip_via_descriptors() {
        for d in runtime_descriptors() {
            assert_eq!(d.runtime.as_slug(), d.slug);
            assert_eq!(d.runtime.to_string(), d.display_name);
            assert!(
                ClawRuntime::from_str_loose(d.slug).is_some(),
                "slug '{}' should resolve via from_str_loose",
                d.slug
            );
            for alias in d.aliases {
                assert_eq!(
                    ClawRuntime::from_str_loose(alias).as_ref(),
                    Some(&d.runtime),
                    "alias '{}' should resolve to {:?}",
                    alias,
                    d.runtime
                );
            }
        }
    }

    #[test]
    fn openfang_uses_config_file_flag() {
        let openfang = runtime_descriptor("openfang").expect("openfang descriptor");
        assert_eq!(
            openfang.config_dir_flag,
            ConfigDirFlag::ConfigFile {
                filename: "config.toml"
            }
        );
    }
}
