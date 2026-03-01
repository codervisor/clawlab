use anyhow::Result;
use clap::{Parser, Subcommand};
use clawden_core::{
    ClawRuntime, ExecutionMode, LifecycleManager, ProcessManager, RuntimeInstaller,
};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Parser)]
#[command(name = "clawden", version, about = "ClawDen orchestration CLI")]
struct Cli {
    #[arg(long, global = true, default_value_t = false)]
    no_docker: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    Init,
    /// Install runtimes for direct execution mode.
    Install {
        runtime: Option<String>,
        #[arg(long)]
        all: bool,
        #[arg(long)]
        list: bool,
    },
    /// Remove a directly installed runtime.
    Uninstall {
        runtime: String,
    },
    /// Start all runtimes from clawden.yaml
    Up {
        /// Specific runtimes to start (starts all if empty)
        runtimes: Vec<String>,
    },
    /// Run a single runtime
    Run {
        runtime: Option<String>,
        /// Channels to connect
        #[arg(long)]
        channel: Vec<String>,
        /// Tools to enable
        #[arg(long = "with")]
        tools: Option<String>,
        /// Restart on failure policy.
        #[arg(long)]
        restart: Option<String>,
    },
    /// Show running runtimes
    Ps,
    /// Stop runtimes
    Stop {
        /// Specific runtime to stop (stops all if empty)
        runtime: Option<String>,
    },
    /// Tail runtime log files.
    Logs {
        runtime: String,
        #[arg(long, default_value_t = 50)]
        lines: usize,
    },
    /// Start local dashboard server and open browser.
    Dashboard {
        #[arg(long, default_value_t = 8080)]
        port: u16,
    },
    /// Check local direct-install prerequisites.
    Doctor,
    /// Channel management
    Channels {
        #[command(subcommand)]
        command: Option<ChannelCommand>,
    },
}

#[derive(Debug, Subcommand)]
enum ChannelCommand {
    /// Test all channel credentials
    Test {
        /// Specific channel type to test
        channel_type: Option<String>,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let installer = RuntimeInstaller::new()?;
    let process_manager = ProcessManager::new(ExecutionMode::Auto)?;
    let registry = clawden_adapters::builtin_registry();
    let mut manager = LifecycleManager::new(registry.adapters_map());

    match cli.command {
        Commands::Init => println!("clawden init scaffold is not implemented yet"),
        Commands::Install { runtime, all, list } => {
            if list {
                let installed = installer.list_installed()?;
                if installed.is_empty() {
                    println!("No runtimes installed");
                } else {
                    for row in installed {
                        println!(
                            "{}\t{}\t{}",
                            row.runtime,
                            row.version,
                            row.executable.display()
                        );
                    }
                }
                return Ok(());
            }

            if all {
                let installed = installer.install_all()?;
                for row in installed {
                    println!(
                        "Installed {}@{} at {}",
                        row.runtime,
                        row.version,
                        row.executable.display()
                    );
                }
                return Ok(());
            }

            let Some(runtime_spec) = runtime else {
                anyhow::bail!("specify a runtime (e.g. clawden install zeroclaw or --all)");
            };

            let (runtime_name, version) = parse_runtime_version(&runtime_spec);
            let installed = installer.install_runtime(&runtime_name, version.as_deref())?;
            println!(
                "Installed {}@{} at {}",
                installed.runtime,
                installed.version,
                installed.executable.display()
            );
        }
        Commands::Uninstall { runtime } => {
            installer.uninstall_runtime(&runtime)?;
            println!("Uninstalled {runtime}");
        }
        Commands::Up { runtimes } => {
            let mode = process_manager.resolve_mode(cli.no_docker || env_no_docker_enabled());
            let target_runtimes = if runtimes.is_empty() {
                installer
                    .list_installed()?
                    .into_iter()
                    .map(|row| row.runtime)
                    .collect::<Vec<_>>()
            } else {
                runtimes
            };

            if target_runtimes.is_empty() {
                println!("No runtimes to start. Install one with: clawden install zeroclaw");
                return Ok(());
            }

            for runtime in target_runtimes {
                match mode {
                    ExecutionMode::Docker => {
                        println!("Docker mode is available; direct processes are not started for {runtime}");
                    }
                    ExecutionMode::Direct | ExecutionMode::Auto => {
                        let executable =
                            installer.runtime_executable(&runtime).ok_or_else(|| {
                                anyhow::anyhow!(
                                    "Runtime '{}' not installed. Run 'clawden install {}' first.",
                                    runtime,
                                    runtime
                                )
                            })?;
                        let info = process_manager.start_direct(&runtime, &executable, &[])?;
                        append_audit_file("runtime.start", &runtime, "ok")?;
                        println!("Started {runtime} (pid {})", info.pid);
                    }
                }
            }
        }
        Commands::Run {
            runtime,
            channel,
            tools,
            restart,
        } => {
            let rt = runtime.unwrap_or_else(|| "zeroclaw".to_string());
            let tools_list = tools
                .map(|t| {
                    t.split(',')
                        .map(|s| s.trim().to_string())
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            println!(
                "Running {} with channels {:?} and tools {:?}",
                rt, channel, tools_list
            );

            let mode = process_manager.resolve_mode(cli.no_docker || env_no_docker_enabled());
            match mode {
                ExecutionMode::Docker => {
                    let runtime = parse_runtime(&rt)?;
                    let record = manager.register_agent(
                        format!("{}-default", runtime.as_slug()),
                        runtime,
                        vec!["chat".to_string()],
                    );
                    manager
                        .start_agent(&record.id)
                        .await
                        .map_err(anyhow::Error::msg)?;
                    println!(
                        "Started {} via core adapter path (docker available, server not required)",
                        rt
                    );
                }
                ExecutionMode::Direct | ExecutionMode::Auto => {
                    let executable = installer.runtime_executable(&rt).ok_or_else(|| {
                        anyhow::anyhow!(
                            "Runtime '{}' not installed. Run 'clawden install {}' to install it.",
                            rt,
                            rt
                        )
                    })?;

                    let mut args = Vec::new();
                    if !channel.is_empty() {
                        args.push(format!("--channels={}", channel.join(",")));
                    }
                    if let Some(policy) = restart {
                        args.push(format!("--restart={policy}"));
                    }

                    let info = process_manager.start_direct(&rt, &executable, &args)?;
                    append_audit_file("runtime.start", &rt, "ok")?;
                    println!(
                        "Started {} in direct mode (pid {}, logs: {})",
                        rt,
                        info.pid,
                        info.log_path.display()
                    );
                }
            }
        }
        Commands::Ps => {
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
        }
        Commands::Stop { runtime } => {
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
        }
        Commands::Logs { runtime, lines } => {
            let logs = process_manager.tail_logs(&runtime, lines)?;
            if logs.is_empty() {
                println!("No logs for {runtime}");
            } else {
                println!("{logs}");
            }
        }
        Commands::Dashboard { port } => {
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
        }
        Commands::Doctor => {
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
        }
        Commands::Channels { command } => match command {
            None => {
                let metadata = manager.list_runtime_metadata();
                for runtime in metadata {
                    println!("{}", runtime.runtime.as_slug());
                    for (channel, support) in runtime.channel_support {
                        println!("  {}: {:?}", channel, support);
                    }
                }
            }
            Some(ChannelCommand::Test { channel_type }) => {
                if let Some(ct) = channel_type {
                    println!(
                        "Channel config test for '{ct}' is available in dashboard server mode"
                    );
                } else {
                    println!("Channel config test requires a channel type");
                }
            }
        },
    }

    Ok(())
}

fn parse_runtime(value: &str) -> Result<ClawRuntime> {
    ClawRuntime::from_str_loose(value).ok_or_else(|| anyhow::anyhow!("unknown runtime: {value}"))
}

fn parse_runtime_version(spec: &str) -> (String, Option<String>) {
    if let Some((runtime, version)) = spec.split_once('@') {
        (runtime.to_string(), Some(version.to_string()))
    } else {
        (spec.to_string(), None)
    }
}

fn command_exists(command: &str) -> bool {
    Command::new("which")
        .arg(command)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn env_no_docker_enabled() -> bool {
    std::env::var("CLAWDEN_NO_DOCKER")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

fn append_audit_file(action: &str, runtime: &str, outcome: &str) -> Result<()> {
    let home = std::env::var("HOME")?;
    let log_dir = PathBuf::from(home).join(".clawden").join("logs");
    std::fs::create_dir_all(&log_dir)?;
    let log_path = log_dir.join("audit.log");
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before UNIX_EPOCH")
        .as_millis();
    let line = format!("{now}\t{action}\t{runtime}\t{outcome}\n");

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)?;
    file.write_all(line.as_bytes())?;
    Ok(())
}
