use anyhow::Result;
use clap::{Parser, Subcommand, ValueEnum};
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};

#[derive(Debug, Parser)]
#[command(name = "clawlab", version, about = "ClawLab orchestration CLI")]
struct Cli {
    #[arg(long, global = true, default_value = "http://127.0.0.1:8080")]
    server_url: String,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    Init,
    Server {
        #[command(subcommand)]
        command: ServerCommand,
    },
    Agent {
        #[command(subcommand)]
        command: AgentCommand,
    },
    Fleet {
        #[command(subcommand)]
        command: FleetCommand,
    },
    Task {
        #[command(subcommand)]
        command: TaskCommand,
    },
    Skill {
        #[command(subcommand)]
        command: SkillCommand,
    },
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },
}

#[derive(Debug, Subcommand)]
enum ServerCommand {
    Start,
}

#[derive(Debug, Subcommand)]
enum AgentCommand {
    Register {
        name: String,
        #[arg(value_enum)]
        runtime: RuntimeArg,
        #[arg(long = "capability")]
        capabilities: Vec<String>,
    },
    List,
    Start { id: String },
    Stop { id: String },
    Health,
}

#[derive(Debug, Subcommand)]
enum FleetCommand {
    Status,
}

#[derive(Debug, Subcommand)]
enum TaskCommand {
    Send {
        message: String,
        #[arg(long)]
        agent_id: Option<String>,
        #[arg(long = "capability")]
        required_capabilities: Vec<String>,
    },
}

#[derive(Debug, Subcommand)]
enum SkillCommand {
    Create { name: String },
    Test { name: String },
    Publish { name: String },
}

#[derive(Debug, Subcommand)]
enum ConfigCommand {
    Set { key: String, value: String },
    Diff,
}

#[derive(Debug, Clone, ValueEnum)]
enum RuntimeArg {
    Openclaw,
    Zeroclaw,
    Picoclaw,
    Nanoclaw,
    Ironclaw,
    Nullclaw,
    Microclaw,
    Mimiclaw,
}

impl RuntimeArg {
    fn as_runtime(&self) -> &'static str {
        match self {
            RuntimeArg::Openclaw => "open-claw",
            RuntimeArg::Zeroclaw => "zero-claw",
            RuntimeArg::Picoclaw => "pico-claw",
            RuntimeArg::Nanoclaw => "nano-claw",
            RuntimeArg::Ironclaw => "iron-claw",
            RuntimeArg::Nullclaw => "null-claw",
            RuntimeArg::Microclaw => "micro-claw",
            RuntimeArg::Mimiclaw => "mimi-claw",
        }
    }
}

#[derive(Debug, Serialize)]
struct RegisterAgentRequest {
    name: String,
    runtime: String,
    capabilities: Vec<String>,
}

#[derive(Debug, Serialize)]
struct SendTaskRequest {
    message: String,
    required_capabilities: Vec<String>,
    agent_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct FleetStatus {
    total_agents: usize,
    running_agents: usize,
    degraded_agents: usize,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let client = Client::new();
    let base = cli.server_url.trim_end_matches('/');

    match cli.command {
        Commands::Init => println!("clawlab init scaffold is not implemented yet"),
        Commands::Server { command } => match command {
            ServerCommand::Start => println!("run: cargo run -p clawlab-server"),
        },
        Commands::Agent { command } => match command {
            AgentCommand::Register {
                name,
                runtime,
                capabilities,
            } => {
                let body = RegisterAgentRequest {
                    name,
                    runtime: runtime.as_runtime().to_string(),
                    capabilities,
                };
                let response = client
                    .post(format!("{base}/agents/register"))
                    .json(&body)
                    .send()?
                    .error_for_status()?;
                println!("{}", response.text()?);
            }
            AgentCommand::List => {
                let response = client
                    .get(format!("{base}/agents"))
                    .send()?
                    .error_for_status()?;
                println!("{}", response.text()?);
            }
            AgentCommand::Start { id } => {
                let response = client
                    .post(format!("{base}/agents/{id}/start"))
                    .send()?
                    .error_for_status()?;
                println!("{}", response.text()?);
            }
            AgentCommand::Stop { id } => {
                let response = client
                    .post(format!("{base}/agents/{id}/stop"))
                    .send()?
                    .error_for_status()?;
                println!("{}", response.text()?);
            }
            AgentCommand::Health => {
                let response = client
                    .get(format!("{base}/agents/health"))
                    .send()?
                    .error_for_status()?;
                println!("{}", response.text()?);
            }
        },
        Commands::Fleet { command } => match command {
            FleetCommand::Status => {
                let response = client
                    .get(format!("{base}/fleet/status"))
                    .send()?
                    .error_for_status()?;
                let status: FleetStatus = response.json()?;
                println!(
                    "fleet: total={}, running={}, degraded={}",
                    status.total_agents, status.running_agents, status.degraded_agents
                );
            }
        },
        Commands::Task { command } => match command {
            TaskCommand::Send {
                message,
                agent_id,
                required_capabilities,
            } => {
                let body = SendTaskRequest {
                    message,
                    required_capabilities,
                    agent_id,
                };
                let response = client
                    .post(format!("{base}/task/send"))
                    .json(&body)
                    .send()?
                    .error_for_status()?;
                println!("{}", response.text()?);
            }
        },
        Commands::Skill { command } => println!("skill command: {command:?}"),
        Commands::Config { command } => println!("config command: {command:?}"),
    }

    Ok(())
}
