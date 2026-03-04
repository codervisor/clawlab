use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(
    name = "clawden",
    version,
    about = "Run and manage claw runtimes from one CLI"
)]
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
        /// Use a named quick-start template
        #[arg(long)]
        template: Option<String>,
        /// Skip interactive prompts and use defaults
        #[arg(long, default_value_t = false)]
        non_interactive: bool,
        /// Assume yes for prompts (CI friendly)
        #[arg(long, default_value_t = false)]
        yes: bool,
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
        #[arg(long, short = 'U')]
        upgrade: bool,
        #[arg(long)]
        outdated: bool,
    },
    /// Remove a directly installed runtime.
    Uninstall { runtime: String },
    /// Start all runtimes from clawden.yaml
    Up {
        /// Specific runtimes to start (starts all if empty)
        runtimes: Vec<String>,
        /// Run in background and return immediately
        #[arg(short = 'd', long, default_value_t = false)]
        detach: bool,
        /// Disable runtime name prefixes in attached log output
        #[arg(long, default_value_t = false)]
        no_log_prefix: bool,
        /// Graceful shutdown timeout in seconds
        #[arg(long, default_value_t = 10)]
        timeout: u64,
    },
    /// Start previously configured runtimes without attaching logs
    Start {
        /// Specific runtimes to start (starts all if empty)
        runtimes: Vec<String>,
    },
    /// Stop all project runtimes and clean up state
    Down {
        /// Specific runtimes to stop (stops all project runtimes if empty)
        runtimes: Vec<String>,
        /// Graceful shutdown timeout in seconds
        #[arg(long, default_value_t = 10)]
        timeout: u64,
        /// Stop project-owned stale runtimes no longer declared in clawden.yaml
        #[arg(long, default_value_t = false)]
        remove_orphans: bool,
    },
    /// Restart runtimes
    Restart {
        /// Specific runtimes to restart (restarts all if empty)
        runtimes: Vec<String>,
        /// Graceful shutdown timeout in seconds
        #[arg(long, default_value_t = 10)]
        timeout: u64,
    },
    /// Run a claw runtime directly
    #[command(trailing_var_arg = true)]
    Run {
        /// Channels to connect (must appear before runtime name)
        #[arg(long)]
        channel: Vec<String>,
        /// Tools to enable (must appear before runtime name)
        #[arg(long = "with")]
        tools: Option<String>,
        /// Remove one-off state after exit
        #[arg(long, default_value_t = false)]
        rm: bool,
        /// Run in background and return immediately
        #[arg(short = 'd', long, default_value_t = false)]
        detach: bool,
        /// Restart on failure policy
        #[arg(long)]
        restart: Option<String>,
        /// Runtime name followed by runtime args
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        runtime_and_args: Vec<String>,
    },
    /// Show running runtimes
    Ps,
    /// Stop runtimes
    Stop {
        /// Specific runtime to stop (stops all if empty)
        runtime: Option<String>,
        /// Graceful shutdown timeout in seconds
        #[arg(long, default_value_t = 10)]
        timeout: u64,
    },
    /// Tail or follow runtime log files.
    Logs {
        /// Follow log output
        #[arg(short = 'f', long, default_value_t = false)]
        follow: bool,
        /// Number of lines to show from end of file
        #[arg(long = "tail", default_value_t = 50)]
        tail: usize,
        /// Prefix each line with a timestamp
        #[arg(long, default_value_t = false)]
        timestamps: bool,
        /// Optional list of runtimes (defaults to all running)
        runtimes: Vec<String>,
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
    /// LLM provider management
    Providers {
        #[command(subcommand)]
        command: Option<ProviderCommand>,
    },
    /// Built-in tool management
    Tools {
        #[command(subcommand)]
        command: ToolCommand,
    },
}

#[cfg(test)]
mod tests {
    use super::{Cli, Commands};
    use clap::Parser;

    #[test]
    fn run_parses_runtime_without_separator() {
        let cli = Cli::try_parse_from([
            "clawden",
            "run",
            "zeroclaw",
            "--verbose",
            "--model",
            "gpt-4",
        ])
        .expect("parse run command");

        match cli.command {
            Commands::Run {
                runtime_and_args,
                channel,
                tools,
                ..
            } => {
                assert!(channel.is_empty());
                assert!(tools.is_none());
                assert_eq!(
                    runtime_and_args,
                    vec![
                        "zeroclaw".to_string(),
                        "--verbose".to_string(),
                        "--model".to_string(),
                        "gpt-4".to_string(),
                    ]
                );
            }
            _ => panic!("expected run command"),
        }
    }

    #[test]
    fn run_parses_clawden_flags_before_runtime() {
        let cli = Cli::try_parse_from([
            "clawden",
            "run",
            "--channel",
            "telegram",
            "--with",
            "web-search",
            "zeroclaw",
            "--help",
        ])
        .expect("parse run command");

        match cli.command {
            Commands::Run {
                runtime_and_args,
                channel,
                tools,
                ..
            } => {
                assert_eq!(channel, vec!["telegram".to_string()]);
                assert_eq!(tools, Some("web-search".to_string()));
                assert_eq!(
                    runtime_and_args,
                    vec!["zeroclaw".to_string(), "--help".to_string()]
                );
            }
            _ => panic!("expected run command"),
        }
    }
}

#[derive(Debug, Subcommand)]
pub enum ChannelCommand {
    /// Test all channel credentials
    Test {
        /// Specific channel type to test
        channel_type: Option<String>,
    },
}

#[derive(Debug, Subcommand)]
pub enum ProviderCommand {
    /// Validate configured provider credentials
    Test {
        /// Optional provider name to test
        provider: Option<String>,
    },
    /// Set a provider API key in local .env
    SetKey {
        /// Provider name (e.g. openai, anthropic, google)
        provider: String,
    },
}

#[derive(Debug, Subcommand)]
pub enum ToolCommand {
    /// List available built-in tools
    List {
        /// Show only installed or activated tools
        #[arg(long, default_value_t = false)]
        installed: bool,
    },
    /// Show detailed metadata for one tool
    Info {
        /// Tool name
        tool: String,
    },
}
