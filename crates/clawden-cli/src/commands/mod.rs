mod channels;
mod config;
mod config_gen;
mod dashboard;
mod doctor;
mod down;
mod init;
mod install;
mod logs;
mod providers;
mod ps;
mod restart;
mod run;
mod start;
mod stop;
mod tools;
mod up;

#[cfg(test)]
use std::sync::{Mutex, OnceLock};

pub use channels::exec_channels;
pub use config::exec_config_env;
pub use config::exec_config_show;
pub use dashboard::exec_dashboard;
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

#[cfg(test)]
pub(crate) fn test_env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}
