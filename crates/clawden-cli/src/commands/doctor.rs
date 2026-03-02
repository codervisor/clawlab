use anyhow::Result;
use clawden_core::{ProcessManager, RuntimeInstaller};

use crate::util::command_exists;

pub fn exec_doctor(installer: &RuntimeInstaller) -> Result<()> {
    println!("docker_available={}", ProcessManager::docker_available());
    println!("node_available={}", command_exists("node"));
    println!("npm_available={}", command_exists("npm"));
    println!("git_available={}", command_exists("git"));
    println!(
        "curl_available={}",
        command_exists("curl") || command_exists("wget")
    );
    println!("clawden_home={}", installer.root_dir().display());
    for row in installer.list_installed()? {
        println!("installed={}@{}", row.runtime, row.version);
    }
    Ok(())
}
