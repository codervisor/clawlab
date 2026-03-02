mod cli;
mod commands;
mod util;

use anyhow::Result;
use clap::Parser;
use cli::{Cli, Commands};
use clawden_core::{ExecutionMode, LifecycleManager, ProcessManager, RuntimeInstaller};

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let installer = RuntimeInstaller::new()?;
    let process_manager = ProcessManager::new(ExecutionMode::Auto)?;
    let registry = clawden_adapters::builtin_registry();
    let mut manager = LifecycleManager::new(registry.adapters_map());

    match cli.command {
        Commands::Init {
            runtime,
            multi,
            template,
            reconfigure,
            non_interactive,
            yes,
            force,
        } => commands::exec_init(commands::InitOptions {
            runtime,
            multi,
            template,
            reconfigure,
            non_interactive,
            yes,
            force,
        })?,
        Commands::Install { runtime, all, list } => {
            commands::exec_install(&installer, runtime, all, list)?
        }
        Commands::Uninstall { runtime } => commands::exec_uninstall(&installer, runtime)?,
        Commands::Up { runtimes } => {
            commands::exec_up(
                runtimes,
                cli.no_docker,
                &installer,
                &process_manager,
                &mut manager,
            )
            .await?
        }
        Commands::Run {
            runtime,
            channel,
            tools,
            restart,
        } => {
            commands::exec_run(
                runtime,
                channel,
                tools,
                restart,
                cli.no_docker,
                &installer,
                &process_manager,
                &mut manager,
            )
            .await?
        }
        Commands::Ps => commands::exec_ps(&process_manager)?,
        Commands::Stop { runtime } => commands::exec_stop(&process_manager, runtime)?,
        Commands::Logs { runtime, lines } => {
            commands::exec_logs(&process_manager, runtime, lines)?
        }
        Commands::Dashboard { port } => commands::exec_dashboard(port)?,
        Commands::Doctor => commands::exec_doctor(&installer)?,
        Commands::Channels { command } => commands::exec_channels(command, &mut manager)?,
        Commands::Providers { command } => commands::exec_providers(command).await?,
    }

    Ok(())
}
