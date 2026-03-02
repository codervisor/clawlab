use anyhow::Result;
use std::process::{Command, Stdio};

use crate::util::command_exists;

pub fn exec_dashboard(port: u16) -> Result<()> {
    let url = format!("http://127.0.0.1:{port}");
    let _ = Command::new("open")
        .arg(&url)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
    println!("Starting dashboard server on {url}");

    let status = if command_exists("clawden-server") {
        Command::new("clawden-server")
            .env("CLAWDEN_SERVER_PORT", port.to_string())
            .status()?
    } else {
        Command::new("cargo")
            .arg("run")
            .arg("-p")
            .arg("clawden-server")
            .env("CLAWDEN_SERVER_PORT", port.to_string())
            .status()?
    };
    if !status.success() {
        anyhow::bail!("clawden-server exited with status {status}");
    }
    Ok(())
}
