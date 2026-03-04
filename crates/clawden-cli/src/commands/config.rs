use anyhow::Result;
use serde_json::json;

use crate::commands::up::load_config_with_env_file;

pub fn exec_config_show(
    runtime: &str,
    format: &str,
    reveal: bool,
    env_file: Option<&str>,
) -> Result<()> {
    let Some(config) = load_config_with_env_file(env_file)? else {
        anyhow::bail!("clawden.yaml not found in current directory");
    };
    let env_vars = super::up::build_runtime_env_vars(&config, runtime)?;

    match format {
        "native" => {
            println!("[runtime]");
            println!("name = \"{runtime}\"");
            println!("\n[env]");
            for (k, v) in env_vars {
                println!("{k} = \"{}\"", maybe_redact(&k, &v, reveal));
            }
        }
        "env" => {
            for (k, v) in env_vars {
                println!("{k}={}", maybe_redact(&k, &v, reveal));
            }
        }
        "json" => {
            let env = env_vars
                .into_iter()
                .map(|(k, v)| (k.clone(), json!(maybe_redact(&k, &v, reveal))))
                .collect::<serde_json::Map<_, _>>();
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({"runtime": runtime, "env": env}))?
            );
        }
        _ => anyhow::bail!("unsupported format '{format}'. Use: native, env, json"),
    }

    Ok(())
}

fn maybe_redact(key: &str, value: &str, reveal: bool) -> String {
    if reveal {
        return value.to_string();
    }
    let upper = key.to_ascii_uppercase();
    if upper.contains("TOKEN") || upper.contains("KEY") || upper.contains("SECRET") {
        return "<redacted>".to_string();
    }
    value.to_string()
}
