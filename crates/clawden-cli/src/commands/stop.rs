use anyhow::Result;
use clawden_core::ProcessManager;

use crate::util::append_audit_file;

pub fn exec_stop(process_manager: &ProcessManager, runtime: Option<String>) -> Result<()> {
    if let Some(rt) = runtime {
        println!("Stopping {}...", rt);
        process_manager.stop(&rt)?;
        append_audit_file("runtime.stop", &rt, "ok")?;
    } else {
        println!("Stopping all runtimes...");
        for status in process_manager.list_statuses()? {
            process_manager.stop(&status.runtime)?;
            append_audit_file("runtime.stop", &status.runtime, "ok")?;
            println!("Stopped {}", status.runtime);
        }
    }
    Ok(())
}
