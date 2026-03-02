use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "clawden", version, about = "ClawDen orchestration CLI")]
pub struct Cli {
    #[arg(long, global = true, default_value_t = false)]
    pub no_docker: bool,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Scaffold a new clawden.yaml project config
    Init {
        /// Runtime to use (default: zeroclaw)
        #[arg(long, default_value = "zeroclaw")]
        runtime: String,
        /// Generate a multi-runtime template instead of single-runtime shorthand
        #[arg(long)]
        multi: bool,
        /// Overwrite existing clawden.yaml
        #[arg(long)]
        force: bool,
    },
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
pub enum ChannelCommand {
    /// Test all channel credentials
    Test {
        /// Specific channel type to test
        channel_type: Option<String>,
    },
}
