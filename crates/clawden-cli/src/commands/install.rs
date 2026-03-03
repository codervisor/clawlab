use anyhow::Result;
use clawden_core::{version_satisfies, RuntimeInstaller};
use indicatif::{ProgressBar, ProgressStyle};
use std::collections::HashMap;
use std::time::Duration;

use super::up::{load_config, pinned_version_for_runtime};
use crate::util::append_audit_file;
use crate::util::parse_runtime_version;

fn install_spinner() -> ProgressBar {
    let spinner = ProgressBar::new_spinner();
    spinner.set_style(
        ProgressStyle::default_spinner()
            .tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏")
            .template("{spinner:.cyan} {msg}")
            .expect("invalid spinner template"),
    );
    spinner.enable_steady_tick(Duration::from_millis(80));
    spinner
}

fn with_progress(installer: &mut RuntimeInstaller) -> ProgressBar {
    let spinner = install_spinner();
    let sp = spinner.clone();
    installer.set_progress_callback(move |msg| {
        sp.set_message(msg.to_string());
    });
    spinner
}

pub fn exec_install(
    installer: &mut RuntimeInstaller,
    runtime: Option<String>,
    all: bool,
    list: bool,
    upgrade: bool,
    outdated: bool,
) -> Result<()> {
    if outdated && (list || all || runtime.is_some() || upgrade) {
        anyhow::bail!("--outdated cannot be combined with runtime, --all, --list, or --upgrade");
    }

    if list {
        let installed = installer.list_installed()?;
        if installed.is_empty() {
            println!("No runtimes installed");
        } else {
            for row in installed {
                println!(
                    "{}\t{}\t{}",
                    row.runtime,
                    row.version,
                    row.executable.display()
                );
            }
        }
        return Ok(());
    }

    if outdated {
        let checks = installer.check_for_updates()?;
        if checks.is_empty() {
            println!("No runtimes installed");
            return Ok(());
        }

        println!(
            "{:<12} {:<12} {:<12} STATUS",
            "RUNTIME", "INSTALLED", "LATEST"
        );
        let mut has_updates = false;
        for row in checks {
            let status = if row.update_available {
                has_updates = true;
                "Update available"
            } else {
                "Up to date"
            };
            println!(
                "{:<12} {:<12} {:<12} {}",
                row.runtime, row.installed, row.latest, status
            );
        }

        if has_updates {
            std::process::exit(1);
        }
        return Ok(());
    }

    if upgrade {
        let pins = pinned_versions_map();
        let mut targets: Vec<(String, Option<String>)> = Vec::new();

        if let Some(runtime_spec) = runtime {
            let (runtime_name, cli_version) = parse_runtime_version(&runtime_spec);
            targets.push((runtime_name, cli_version));
        } else {
            let installed = installer.list_installed()?;
            if installed.is_empty() {
                println!("No installed runtimes to upgrade");
                return Ok(());
            }
            for row in installed {
                targets.push((row.runtime, None));
            }
        }

        let mut changed = 0usize;
        for (runtime_name, cli_version) in targets {
            let request = cli_version
                .as_deref()
                .or_else(|| pins.get(&runtime_name).map(String::as_str));
            let current = installer.installed_version(&runtime_name)?;
            let target =
                resolve_upgrade_target(installer, &runtime_name, request, current.as_deref())?;

            if current.as_deref() == Some(target.as_str()) {
                println!("{runtime_name} already up to date ({target})");
                continue;
            }

            let spinner = with_progress(installer);
            let installed = installer.install_runtime(&runtime_name, Some(&target))?;
            spinner.finish_and_clear();
            let _ = append_audit_file("runtime.upgrade", &runtime_name, "ok");
            if let Some(prev) = current {
                println!(
                    "Upgraded {} {} -> {}",
                    runtime_name, prev, installed.version
                );
            } else {
                println!("Installed {}@{}", runtime_name, installed.version);
            }
            changed += 1;
        }

        if changed == 0 {
            println!("All selected runtimes are already up to date");
        }
        return Ok(());
    }

    if all {
        let spinner = with_progress(installer);
        let installed = installer.install_all()?;
        spinner.finish_and_clear();
        for row in installed {
            println!(
                "Installed {}@{} at {}",
                row.runtime,
                row.version,
                row.executable.display()
            );
        }
        return Ok(());
    }

    let Some(runtime_spec) = runtime else {
        anyhow::bail!("specify a runtime (e.g. clawden install zeroclaw or --all)");
    };

    let (runtime_name, version) = parse_runtime_version(&runtime_spec);
    let spinner = with_progress(installer);
    let installed = installer.install_runtime(&runtime_name, version.as_deref())?;
    spinner.finish_and_clear();
    println!(
        "Installed {}@{} at {}",
        installed.runtime,
        installed.version,
        installed.executable.display()
    );
    Ok(())
}

fn pinned_versions_map() -> HashMap<String, String> {
    let Ok(Some(config)) = load_config() else {
        return HashMap::new();
    };

    let mut map = HashMap::new();
    if let Some(single_runtime) = &config.runtime {
        if let Some(pin) = pinned_version_for_runtime(&config, single_runtime) {
            map.insert(single_runtime.clone(), pin.to_string());
        }
    }
    for runtime in &config.runtimes {
        if let Some(pin) = pinned_version_for_runtime(&config, &runtime.name) {
            map.insert(runtime.name.clone(), pin.to_string());
        }
    }
    map
}

fn resolve_upgrade_target(
    installer: &RuntimeInstaller,
    runtime: &str,
    request: Option<&str>,
    installed: Option<&str>,
) -> Result<String> {
    let req = request.unwrap_or("latest").trim();
    if req.is_empty() || req.eq_ignore_ascii_case("latest") {
        return installer.query_latest_version(runtime);
    }

    if is_constraint(req) {
        let latest = installer.query_latest_version(runtime)?;
        if version_satisfies(&latest, req) {
            return Ok(latest);
        }
        if let Some(installed_version) = installed {
            if version_satisfies(installed_version, req) {
                return Ok(installed_version.to_string());
            }
        }
        anyhow::bail!("No version available for '{runtime}' matching constraint '{req}'");
    }

    Ok(req.trim_start_matches('v').to_string())
}

fn is_constraint(value: &str) -> bool {
    let trimmed = value.trim();
    trimmed.ends_with(".x")
        || trimmed.ends_with(".*")
        || trimmed.starts_with('>')
        || trimmed.starts_with('<')
        || trimmed.starts_with('=')
}

pub fn exec_uninstall(installer: &RuntimeInstaller, runtime: String) -> Result<()> {
    installer.uninstall_runtime(&runtime)?;
    println!("Uninstalled {runtime}");
    Ok(())
}
