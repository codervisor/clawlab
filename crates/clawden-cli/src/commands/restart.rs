use anyhow::Result;
use clawden_core::{LifecycleManager, ProcessManager, RuntimeInstaller};

use crate::commands::{
    stop::exec_stop,
    up::{exec_up, UpOptions},
};
use crate::util::append_audit_file;

pub async fn exec_restart(
    runtimes: Vec<String>,
    timeout: u64,
    no_docker: bool,
    installer: &RuntimeInstaller,
    process_manager: &ProcessManager,
    manager: &mut LifecycleManager,
) -> Result<()> {
    if runtimes.is_empty() {
        exec_stop(process_manager, None, timeout)?;
    } else {
        for runtime in &runtimes {
            exec_stop(process_manager, Some(runtime.clone()), timeout)?;
        }
    }

    exec_up(
        UpOptions {
            runtimes: runtimes.clone(),
            env_vars: Vec::new(),
            env_file: None,
            allow_missing_credentials: false,
            detach: true,
            no_log_prefix: false,
            timeout,
        },
        no_docker,
        installer,
        process_manager,
        manager,
    )
    .await?;

    if runtimes.is_empty() {
        for status in process_manager.list_statuses()? {
            append_audit_file("runtime.restart", &status.runtime, "ok")?;
        }
    } else {
        for runtime in &runtimes {
            append_audit_file("runtime.restart", runtime, "ok")?;
        }
    }

    Ok(())
}
