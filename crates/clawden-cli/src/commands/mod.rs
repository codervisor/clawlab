mod channels;
mod config;
mod config_gen;
mod dashboard;
mod docker;
mod doctor;
mod down;
mod feishu;
mod init;
mod install;
mod logs;
mod providers;
mod ps;
mod restart;
mod run;
mod start;
mod stop;
mod telegram;
mod tools;
mod up;
mod workspace;

#[cfg(test)]
use std::sync::{Mutex, OnceLock};

pub use channels::exec_channels;
pub use config::exec_config_env;
pub use config::exec_config_show;
pub use dashboard::exec_dashboard;
pub use docker::exec_docker;
pub use doctor::exec_doctor;
pub use down::exec_down;
pub use init::{exec_init, InitOptions};
pub use install::{exec_install, exec_uninstall};
pub use logs::exec_logs;
pub use providers::exec_providers;
pub use ps::exec_ps;
pub use restart::exec_restart;
pub use run::{exec_run, RunOptions};
pub use start::exec_start;
pub use stop::exec_stop;
pub use tools::exec_tools;
pub use up::{exec_up, UpOptions};
pub use workspace::exec_workspace;

pub(crate) fn load_default_env() {
    let Ok(current_dir) = std::env::current_dir() else {
        return;
    };
    let env_path = current_dir.join(".env");
    if env_path.exists() {
        let _ = dotenvy::from_path(&env_path);
    }
}

#[cfg(test)]
pub(crate) fn test_env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}
