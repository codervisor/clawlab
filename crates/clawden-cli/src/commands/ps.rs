use anyhow::Result;
use clawden_core::ProcessManager;

pub fn exec_ps(process_manager: &ProcessManager) -> Result<()> {
    let statuses = process_manager.list_statuses()?;
    if statuses.is_empty() {
        println!("No running runtimes");
    } else {
        println!(
            "{:<14} {:<8} {:<10} {:<10} {:<10} LOG",
            "RUNTIME", "PID", "MODE", "STATE", "HEALTH"
        );
        for status in statuses {
            println!(
                "{:<14} {:<8} {:<10} {:<10} {:<10} {}",
                status.runtime,
                status
                    .pid
                    .map(|pid| pid.to_string())
                    .unwrap_or_else(|| "-".to_string()),
                format!("{:?}", status.mode),
                if status.running { "running" } else { "stopped" },
                status.health,
                status.log_path.display(),
            );
        }
    }
    Ok(())
}
