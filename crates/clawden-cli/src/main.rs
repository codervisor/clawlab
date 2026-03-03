mod cli;
mod commands;
mod util;

use anyhow::Result;
use clap::Parser;
use clawden_core::{ExecutionMode, LifecycleManager, ProcessManager, RuntimeInstaller};
use cli::{Cli, Commands};

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let mut installer = RuntimeInstaller::new()?;
    let process_manager = ProcessManager::new(ExecutionMode::Auto)?;
    let registry = clawden_adapters::builtin_registry();
    let mut manager = LifecycleManager::new(registry.adapters_map());

    match cli.command {
        Commands::Init {
            runtime,
            multi,
            template,
            non_interactive,
            yes,
            force,
        } => commands::exec_init(commands::InitOptions {
            runtime,
            multi,
            template,
            non_interactive,
            yes,
            force,
        })?,
        Commands::Install {
            runtime,
            all,
            list,
            upgrade,
            outdated,
        } => commands::exec_install(&mut installer, runtime, all, list, upgrade, outdated)?,
        Commands::Uninstall { runtime } => commands::exec_uninstall(&installer, runtime)?,
        Commands::Up {
            runtimes,
            detach,
            no_log_prefix,
            timeout,
        } => {
            commands::exec_up(
                commands::UpOptions {
                    runtimes,
                    detach,
                    no_log_prefix,
                    timeout,
                },
                cli.no_docker,
                &installer,
                &process_manager,
                &mut manager,
            )
            .await?
        }
        Commands::Start { runtimes } => {
            commands::exec_start(
                runtimes,
                cli.no_docker,
                &installer,
                &process_manager,
                &mut manager,
            )
            .await?
        }
        Commands::Down {
            runtimes,
            timeout,
            remove_orphans,
        } => commands::exec_down(&process_manager, runtimes, timeout, remove_orphans)?,
        Commands::Restart { runtimes, timeout } => {
            commands::exec_restart(
                runtimes,
                timeout,
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
            rm,
            detach,
            restart,
            args,
        } => {
            commands::exec_run(
                commands::RunOptions {
                    runtime,
                    channel,
                    tools,
                    restart,
                    rm,
                    detach,
                    extra_args: args,
                    no_docker: cli.no_docker,
                },
                &installer,
                &process_manager,
                &mut manager,
            )
            .await?
        }
        Commands::Ps => commands::exec_ps(&process_manager)?,
        Commands::Stop { runtime, timeout } => {
            commands::exec_stop(&process_manager, runtime, timeout)?
        }
        Commands::Logs {
            follow,
            tail,
            timestamps,
            runtimes,
        } => commands::exec_logs(&process_manager, runtimes, tail, follow, timestamps).await?,
        Commands::Dashboard { port } => commands::exec_dashboard(port)?,
        Commands::Doctor => commands::exec_doctor(&installer)?,
        Commands::Channels { command } => commands::exec_channels(command, &mut manager)?,
        Commands::Providers { command } => commands::exec_providers(command).await?,
        Commands::Tools { command } => commands::exec_tools(command)?,
    }

    Ok(())
}
