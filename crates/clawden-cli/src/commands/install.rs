use anyhow::Result;
use clawden_core::RuntimeInstaller;

use crate::util::parse_runtime_version;

pub fn exec_install(
    installer: &RuntimeInstaller,
    runtime: Option<String>,
    all: bool,
    list: bool,
) -> Result<()> {
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

    if all {
        let installed = installer.install_all()?;
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
    let installed = installer.install_runtime(&runtime_name, version.as_deref())?;
    println!(
        "Installed {}@{} at {}",
        installed.runtime,
        installed.version,
        installed.executable.display()
    );
    Ok(())
}

pub fn exec_uninstall(installer: &RuntimeInstaller, runtime: String) -> Result<()> {
    installer.uninstall_runtime(&runtime)?;
    println!("Uninstalled {runtime}");
    Ok(())
}
