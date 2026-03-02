use anyhow::Result;
use clawden_config::{ClawDenYaml, ProviderRefYaml};
use std::collections::BTreeSet;
use std::io::{self, IsTerminal, Write};

use crate::util::{append_audit_file, parse_runtime};

#[derive(Debug, Clone)]
pub struct InitOptions {
    pub runtime: String,
    pub multi: bool,
    pub template: Option<String>,
    pub reconfigure: bool,
    pub non_interactive: bool,
    pub yes: bool,
    pub force: bool,
}

#[derive(Debug, Clone, Copy)]
enum TemplateKind {
    TelegramBot,
    DiscordBot,
    ApiOnly,
    MultiRuntime,
}

#[derive(Debug, Clone)]
struct WizardSelection {
    runtime: String,
    multi: bool,
    provider: Option<String>,
    model: Option<String>,
    channels: Vec<String>,
    tools: Vec<String>,
}

pub fn exec_init(options: InitOptions) -> Result<()> {
    let yaml_path = std::env::current_dir()?.join("clawden.yaml");
    if yaml_path.exists() && !options.force && !options.reconfigure {
        anyhow::bail!("clawden.yaml already exists. Use --reconfigure or --force.");
    }

    let _ = parse_runtime(&options.runtime)?;

    let template = options
        .template
        .as_deref()
        .map(parse_template)
        .transpose()?;

    let mut selection = load_existing_selection(&yaml_path, &options)?;
    let interactive = !options.non_interactive && !options.yes && io::stdin().is_terminal();

    if interactive && template.is_none() {
        selection = run_wizard(selection)?;
    }

    let yaml_content = if let Some(kind) = template {
        render_template(kind, &selection.runtime)
    } else {
        render_wizard_yaml(&selection)
    };

    std::fs::write(&yaml_path, &yaml_content)?;
    println!("Created {}", yaml_path.display());

    let env_path = yaml_path.parent().unwrap().join(".env");
    ensure_env_file(&env_path, template, &selection)?;

    append_audit_file("project.init", &selection.runtime, "ok")?;
    Ok(())
}

fn load_existing_selection(yaml_path: &std::path::Path, options: &InitOptions) -> Result<WizardSelection> {
    if options.reconfigure && yaml_path.exists() {
        let mut parsed = ClawDenYaml::from_file(yaml_path).map_err(anyhow::Error::msg)?;
        let _ = parsed.resolve_env_vars();

        let runtime = parsed
            .runtime
            .clone()
            .or_else(|| parsed.runtimes.first().map(|rt| rt.name.clone()))
            .unwrap_or_else(|| options.runtime.clone());
        let multi = !parsed.runtimes.is_empty();
        let channels = if let Some(first_rt) = parsed.runtimes.first() {
            first_rt.channels.clone()
        } else {
            Vec::new()
        };
        let tools = if let Some(first_rt) = parsed.runtimes.first() {
            first_rt.tools.clone()
        } else if !parsed.tools.is_empty() {
            parsed.tools.clone()
        } else {
            vec!["git".to_string(), "http".to_string()]
        };
        let provider = if let Some(first_rt) = parsed.runtimes.first() {
            first_rt.provider.clone()
        } else {
            match parsed.provider {
                Some(ProviderRefYaml::Name(name)) => Some(name),
                Some(ProviderRefYaml::Inline(entry)) => entry
                    .provider_type
                    .map(|pt| format!("{:?}", pt).to_ascii_lowercase()),
                None => None,
            }
        };
        let model = parsed
            .model
            .clone()
            .or_else(|| parsed.runtimes.first().and_then(|rt| rt.model.clone()));

        return Ok(WizardSelection {
            runtime,
            multi,
            provider,
            model,
            channels,
            tools,
        });
    }

    Ok(WizardSelection {
        runtime: options.runtime.clone(),
        multi: options.multi,
        provider: Some("openai".to_string()),
        model: Some("gpt-4o-mini".to_string()),
        channels: Vec::new(),
        tools: vec!["git".to_string(), "http".to_string()],
    })
}

fn run_wizard(mut selection: WizardSelection) -> Result<WizardSelection> {
    println!("Welcome to ClawDen! Let's set up your project.\n");

    println!("Step 1/5 - Execution Mode");
    let _docker_mode = prompt_select(
        "How do you want to run claw runtimes?",
        &["Docker (recommended)", "Direct install (no Docker)"],
        0,
    )?;

    println!("\nStep 2/5 - Runtime Selection");
    let runtime = prompt_input("Runtime", &selection.runtime)?;
    let _ = parse_runtime(&runtime)?;
    selection.runtime = runtime;
    selection.multi = prompt_confirm("Use multi-runtime config?", selection.multi)?;

    println!("\nStep 3/5 - Channel Configuration");
    selection.channels = prompt_multiselect(
        "Select channels (comma-separated numbers, empty for none)",
        &["telegram", "discord", "slack"],
        &selection.channels,
    )?;

    println!("\nStep 4/5 - LLM Provider");
    print_detected_provider_envs();
    let provider_idx = prompt_select(
        "Choose provider",
        &[
            "openai",
            "anthropic",
            "google",
            "openrouter",
            "local (openai-compatible)",
            "skip",
        ],
        0,
    )?;
    selection.provider = match provider_idx {
        0 => Some("openai".to_string()),
        1 => Some("anthropic".to_string()),
        2 => Some("google".to_string()),
        3 => Some("openrouter".to_string()),
        4 => Some("local-llm".to_string()),
        _ => None,
    };

    println!("\nStep 5/5 - Tools");
    selection.tools = prompt_multiselect(
        "Select built-in tools (comma-separated numbers)",
        &["git", "http", "core-utils", "python", "code-tools", "database"],
        &selection.tools,
    )?;
    if selection.tools.is_empty() {
        selection.tools = vec!["git".to_string(), "http".to_string()];
    }

    println!("\nSetup complete. Generating files...");
    Ok(selection)
}

fn render_wizard_yaml(selection: &WizardSelection) -> String {
    let mut yaml = String::from("# ClawDen config\n# Docs: https://github.com/codervisor/clawden\n\n");

    if selection.multi {
        yaml.push_str("channels:\n");
        if selection.channels.is_empty() {
            yaml.push_str("  {}\n");
        } else {
            for ch in &selection.channels {
                let env_name = match ch.as_str() {
                    "telegram" => "TELEGRAM_BOT_TOKEN",
                    "discord" => "DISCORD_BOT_TOKEN",
                    "slack" => "SLACK_BOT_TOKEN",
                    _ => "CHANNEL_TOKEN",
                };
                yaml.push_str(&format!("  {ch}:\n    type: {ch}\n    token: ${env_name}\n"));
            }
        }

        if let Some(name) = &selection.provider {
            yaml.push_str("\nproviders:\n");
            yaml.push_str(&format!(
                "  {name}:\n    type: {}\n    api_key: ${}\n",
                provider_type_for_name(name),
                env_var_for_provider(name).unwrap_or("OPENAI_API_KEY")
            ));
        }

        yaml.push_str("\nruntimes:\n");
        yaml.push_str(&format!("  - name: {}\n", selection.runtime));
        if selection.channels.is_empty() {
            yaml.push_str("    channels: []\n");
        } else {
            yaml.push_str(&format!("    channels: [{}]\n", selection.channels.join(", ")));
        }
        yaml.push_str(&format!("    tools: [{}]\n", selection.tools.join(", ")));
        if let Some(provider) = &selection.provider {
            yaml.push_str(&format!("    provider: {provider}\n"));
        }
        if let Some(model) = &selection.model {
            yaml.push_str(&format!("    model: {model}\n"));
        }
    } else {
        yaml.push_str(&format!("runtime: {}\n\n", selection.runtime));
        if selection.channels.is_empty() {
            yaml.push_str("channels: {}\n\n");
        } else {
            yaml.push_str("channels:\n");
            for ch in &selection.channels {
                let env_name = match ch.as_str() {
                    "telegram" => "TELEGRAM_BOT_TOKEN",
                    "discord" => "DISCORD_BOT_TOKEN",
                    "slack" => "SLACK_BOT_TOKEN",
                    _ => "CHANNEL_TOKEN",
                };
                yaml.push_str(&format!("  {ch}:\n    type: {ch}\n    token: ${env_name}\n"));
            }
            yaml.push('\n');
        }

        yaml.push_str("tools:\n");
        for tool in &selection.tools {
            yaml.push_str(&format!("  - {tool}\n"));
        }

        if let Some(name) = &selection.provider {
            yaml.push_str("\nproviders:\n");
            yaml.push_str(&format!(
                "  {name}:\n    type: {}\n    api_key: ${}\n",
                provider_type_for_name(name),
                env_var_for_provider(name).unwrap_or("OPENAI_API_KEY")
            ));
            yaml.push_str(&format!("\nprovider: {name}\n"));
        }
        if let Some(model) = &selection.model {
            yaml.push_str(&format!("model: {model}\n"));
        }
    }

    yaml
}

fn parse_template(template: &str) -> Result<TemplateKind> {
    match template {
        "telegram-bot" => Ok(TemplateKind::TelegramBot),
        "discord-bot" => Ok(TemplateKind::DiscordBot),
        "api-only" => Ok(TemplateKind::ApiOnly),
        "multi-runtime" => Ok(TemplateKind::MultiRuntime),
        _ => anyhow::bail!("unknown template: {template}"),
    }
}

fn render_template(template: TemplateKind, runtime: &str) -> String {
    match template {
        TemplateKind::TelegramBot => format!(
            "runtime: {runtime}\n\
             channels:\n\
               telegram:\n\
                 type: telegram\n\
                 token: $TELEGRAM_BOT_TOKEN\n\
             provider: openai\n\
             model: gpt-4o-mini\n\
             tools: [git, http]\n"
        ),
        TemplateKind::DiscordBot => format!(
            "runtime: {runtime}\n\
             channels:\n\
               discord:\n\
                 type: discord\n\
                 token: $DISCORD_BOT_TOKEN\n\
             provider: openai\n\
             model: gpt-4o-mini\n\
             tools: [git, http]\n"
        ),
        TemplateKind::ApiOnly => format!(
            "runtime: {runtime}\n\
             channels: {{}}\n\
             provider: openai\n\
             model: gpt-4o-mini\n\
             tools: [git, http]\n"
        ),
        TemplateKind::MultiRuntime => {
            "channels: {}\nproviders:\n  openai:\n    type: openai\n    api_key: $OPENAI_API_KEY\nruntimes:\n  - name: zeroclaw\n    channels: []\n    tools: [git, http]\n    provider: openai\n    model: gpt-4o-mini\n  - name: nanoclaw\n    channels: []\n    tools: [git, http]\n    provider: openai\n    model: gpt-4o-mini\n".to_string()
        }
    }
}

fn ensure_env_file(
    env_path: &std::path::Path,
    template: Option<TemplateKind>,
    selection: &WizardSelection,
) -> Result<()> {
    let mut required = BTreeSet::new();
    for channel in &selection.channels {
        match channel.as_str() {
            "telegram" => {
                required.insert("TELEGRAM_BOT_TOKEN".to_string());
            }
            "discord" => {
                required.insert("DISCORD_BOT_TOKEN".to_string());
            }
            "slack" => {
                required.insert("SLACK_BOT_TOKEN".to_string());
            }
            _ => {}
        }
    }
    if let Some(provider) = &selection.provider {
        if let Some(name) = env_var_for_provider(provider) {
            required.insert(name.to_string());
        }
    }
    if matches!(template, Some(TemplateKind::TelegramBot)) {
        required.insert("TELEGRAM_BOT_TOKEN".to_string());
        required.insert("OPENAI_API_KEY".to_string());
    }
    if matches!(template, Some(TemplateKind::DiscordBot)) {
        required.insert("DISCORD_BOT_TOKEN".to_string());
        required.insert("OPENAI_API_KEY".to_string());
    }
    if matches!(template, Some(TemplateKind::ApiOnly | TemplateKind::MultiRuntime)) {
        required.insert("OPENAI_API_KEY".to_string());
    }

    let mut content = if env_path.exists() {
        std::fs::read_to_string(env_path)?
    } else {
        String::from("# ClawDen environment variables\n")
    };

    for key in required {
        if !content.contains(&format!("{key}=")) {
            content.push_str(&format!("{key}=\n"));
        }
    }

    std::fs::write(env_path, content)?;
    println!("Created {}", env_path.display());
    Ok(())
}

fn provider_type_for_name(name: &str) -> &str {
    if name == "local-llm" {
        "openai"
    } else {
        name
    }
}

fn env_var_for_provider(provider: &str) -> Option<&'static str> {
    match provider {
        "openai" => Some("OPENAI_API_KEY"),
        "anthropic" => Some("ANTHROPIC_API_KEY"),
        "google" => Some("GEMINI_API_KEY"),
        "openrouter" => Some("OPENROUTER_API_KEY"),
        _ => None,
    }
}

fn print_detected_provider_envs() {
    let checks = [
        ("OPENAI_API_KEY", std::env::var("OPENAI_API_KEY").is_ok()),
        (
            "ANTHROPIC_API_KEY",
            std::env::var("ANTHROPIC_API_KEY").is_ok(),
        ),
        (
            "OPENROUTER_API_KEY",
            std::env::var("OPENROUTER_API_KEY").is_ok(),
        ),
        (
            "GEMINI_API_KEY / GOOGLE_API_KEY",
            std::env::var("GEMINI_API_KEY").is_ok() || std::env::var("GOOGLE_API_KEY").is_ok(),
        ),
    ];
    println!("Detected API keys in environment:");
    for (name, present) in checks {
        let marker = if present { "x" } else { " " };
        println!("  [{marker}] {name}");
    }
}

fn prompt_select(prompt: &str, options: &[&str], default: usize) -> Result<usize> {
    println!("{prompt}");
    for (idx, option) in options.iter().enumerate() {
        println!("  {}. {}", idx + 1, option);
    }
    let raw = prompt_input("Choose number", &(default + 1).to_string())?;
    Ok(raw
        .parse::<usize>()
        .ok()
        .and_then(|v| v.checked_sub(1))
        .filter(|idx| *idx < options.len())
        .unwrap_or(default))
}

fn prompt_input(prompt: &str, default: &str) -> Result<String> {
    print!("{prompt} [{default}]: ");
    io::stdout().flush()?;
    let mut buffer = String::new();
    io::stdin().read_line(&mut buffer)?;
    let value = buffer.trim();
    Ok(if value.is_empty() {
        default.to_string()
    } else {
        value.to_string()
    })
}

fn prompt_confirm(prompt: &str, default: bool) -> Result<bool> {
    let suffix = if default { "Y/n" } else { "y/N" };
    let value = prompt_input(&format!("{prompt} ({suffix})"), "")?;
    if value.is_empty() {
        return Ok(default);
    }
    Ok(matches!(value.to_ascii_lowercase().as_str(), "y" | "yes"))
}

fn prompt_multiselect(prompt: &str, options: &[&str], current: &[String]) -> Result<Vec<String>> {
    println!("{prompt}");
    for (idx, option) in options.iter().enumerate() {
        println!("  {}. {}", idx + 1, option);
    }
    let defaults = current.join(",");
    let raw = prompt_input("Choices", &defaults)?;
    if raw.trim().is_empty() {
        return Ok(current.to_vec());
    }

    let mut values = Vec::new();
    for token in raw.split(',') {
        let trimmed = token.trim();
        if let Ok(idx) = trimmed.parse::<usize>() {
            if let Some(name) = options.get(idx.saturating_sub(1)) {
                values.push((*name).to_string());
            }
        } else if options.iter().any(|item| item == &trimmed) {
            values.push(trimmed.to_string());
        }
    }
    values.sort();
    values.dedup();
    Ok(values)
}
