use anyhow::Result;
use clawden_config::ClawDenYaml;
use clawden_core::{ProcessManager, RuntimeInstaller};

use crate::util::command_exists;

pub fn exec_doctor(installer: &RuntimeInstaller) -> Result<()> {
    println!("Prerequisites");
    println!("  docker ............... {}", yes_no(ProcessManager::docker_available()));
    println!("  node ................. {}", yes_no(command_exists("node")));
    println!("  npm .................. {}", yes_no(command_exists("npm")));
    println!("  git .................. {}", yes_no(command_exists("git")));
    println!(
        "  curl/wget ............ {}",
        yes_no(command_exists("curl") || command_exists("wget"))
    );
    println!("  clawden_home ......... {}", installer.root_dir().display());

    let yaml_path = std::env::current_dir()?.join("clawden.yaml");
    if yaml_path.exists() {
        println!("\nConfiguration ({})", yaml_path.display());
        let mut config = ClawDenYaml::from_file(&yaml_path).map_err(anyhow::Error::msg)?;
        match config.validate() {
            Ok(()) => println!("  schema ............... ok"),
            Err(errs) => {
                println!("  schema ............... fail");
                for err in errs {
                    println!("    - {err}");
                }
            }
        }

        match config.resolve_env_vars() {
            Ok(()) => println!("  env resolution ....... ok"),
            Err(errs) => {
                println!("  env resolution ....... fail");
                for err in errs {
                    println!("    - {err}");
                }
            }
        }

        if config.providers.is_empty() {
            println!("  providers ............ none configured");
        } else {
            for (name, provider) in &config.providers {
                let key_state = if provider.api_key.is_some() {
                    "ok"
                } else {
                    "missing api_key"
                };
                println!("  provider.{name} ....... {key_state}");
            }
        }
    } else {
        println!("\nConfiguration\n  clawden.yaml .......... missing");
    }

    println!("\nRuntimes");
    let installed = installer.list_installed()?;
    if installed.is_empty() {
        println!("  installed ............ none");
    }
    for row in installed {
        println!("  {} ............. {}", row.runtime, row.version);
    }
    Ok(())
}

fn yes_no(value: bool) -> &'static str {
    if value {
        "ok"
    } else {
        "missing"
    }
}
