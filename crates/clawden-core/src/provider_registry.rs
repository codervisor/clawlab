#[derive(Debug, Clone, Copy)]
pub struct ProviderDescriptor {
    pub name: &'static str,
    pub display_name: &'static str,
    pub env_vars: &'static [&'static str],
    pub test_base_url: &'static str,
}

pub static PROVIDERS: &[ProviderDescriptor] = &[
    ProviderDescriptor {
        name: "openrouter",
        display_name: "OpenRouter",
        env_vars: &["OPENROUTER_API_KEY"],
        test_base_url: "https://openrouter.ai/api/v1",
    },
    ProviderDescriptor {
        name: "openai",
        display_name: "OpenAI",
        env_vars: &["OPENAI_API_KEY"],
        test_base_url: "https://api.openai.com/v1",
    },
    ProviderDescriptor {
        name: "anthropic",
        display_name: "Anthropic",
        env_vars: &["ANTHROPIC_API_KEY"],
        test_base_url: "https://api.anthropic.com",
    },
    ProviderDescriptor {
        name: "google",
        display_name: "Google Gemini",
        env_vars: &["GEMINI_API_KEY", "GOOGLE_API_KEY"],
        test_base_url: "https://generativelanguage.googleapis.com/v1beta",
    },
    ProviderDescriptor {
        name: "mistral",
        display_name: "Mistral",
        env_vars: &["MISTRAL_API_KEY"],
        test_base_url: "https://api.mistral.ai/v1",
    },
    ProviderDescriptor {
        name: "groq",
        display_name: "Groq",
        env_vars: &["GROQ_API_KEY"],
        test_base_url: "https://api.groq.com/openai/v1",
    },
];

pub fn provider_descriptors() -> &'static [ProviderDescriptor] {
    PROVIDERS
}

pub fn provider_descriptor(name: &str) -> Option<&'static ProviderDescriptor> {
    let needle = name.trim().to_ascii_lowercase();
    PROVIDERS
        .iter()
        .find(|descriptor| descriptor.name == needle)
}

pub fn provider_primary_env_var(name: &str) -> Option<&'static str> {
    provider_descriptor(name).and_then(|descriptor| descriptor.env_vars.first().copied())
}

pub fn provider_env_vars(name: &str) -> &'static [&'static str] {
    provider_descriptor(name)
        .map(|descriptor| descriptor.env_vars)
        .unwrap_or(&[])
}

pub fn provider_env_candidates() -> Vec<(&'static str, &'static str)> {
    let mut pairs = Vec::new();
    for descriptor in PROVIDERS {
        for env_var in descriptor.env_vars {
            pairs.push((*env_var, descriptor.name));
        }
    }
    pairs
}

pub fn known_provider_env_vars() -> Vec<&'static str> {
    let mut vars = Vec::new();
    for descriptor in PROVIDERS {
        for env_var in descriptor.env_vars {
            if !vars.contains(env_var) {
                vars.push(*env_var);
            }
        }
    }
    vars
}

pub fn infer_provider_from_host_env() -> Option<(&'static str, &'static str)> {
    for (env_var, provider_name) in provider_env_candidates() {
        if let Ok(val) = std::env::var(env_var) {
            if !val.trim().is_empty() {
                return Some((provider_name, env_var));
            }
        }
    }
    None
}
