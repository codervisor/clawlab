use anyhow::Result;
use clawden_core::ProcessManager;

pub fn exec_logs(process_manager: &ProcessManager, runtime: String, lines: usize) -> Result<()> {
    let logs = process_manager.tail_logs(&runtime, lines)?;
    if logs.is_empty() {
        println!("No logs for {runtime}");
    } else {
        println!("{logs}");
    }
    Ok(())
}
