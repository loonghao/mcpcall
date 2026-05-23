use std::path::PathBuf;

use anyhow::{Result, bail};
use clap::{Args, Parser, Subcommand};
use mcpcall_core::{Endpoint, TransportOptions, parse_key_values};

#[derive(Debug, Parser)]
#[command(
    name = "mcpcall",
    version,
    about = "Call MCP servers from the command line",
    long_about = "mcpcall is a small Rust CLI for listing and calling tools on any MCP server over stdio or Streamable HTTP."
)]
pub struct Cli {
    #[arg(short, long, global = true, action = clap::ArgAction::Count)]
    pub verbose: u8,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// List tools exposed by an MCP server.
    List(ListArgs),
    /// Call one tool exposed by an MCP server.
    Call(CallArgs),
    /// List or read resources exposed by an MCP server.
    Resources(ResourcesArgs),
    /// List or get prompts exposed by an MCP server.
    Prompts(PromptsArgs),
}

#[derive(Debug, Args, Clone)]
pub struct TransportArgs {
    /// Streamable HTTP MCP endpoint, for example http://127.0.0.1:8765/mcp.
    #[arg(
        long,
        alias = "http-url",
        alias = "mcp-url",
        env = "MCP_URL",
        conflicts_with = "stdio"
    )]
    pub url: Option<String>,

    /// Stdio MCP server command, for example "python -m my_server".
    #[arg(long, value_name = "COMMAND", conflicts_with = "url")]
    pub stdio: Option<String>,

    /// Working directory for a stdio server command.
    #[arg(long, value_name = "DIR", requires = "stdio")]
    pub cwd: Option<PathBuf>,

    /// Environment variable for a stdio server, in KEY=VALUE form.
    #[arg(long = "env", value_name = "KEY=VALUE", requires = "stdio")]
    pub env: Vec<String>,

    /// Extra HTTP header for Streamable HTTP, in KEY=VALUE form.
    #[arg(long = "header", value_name = "KEY=VALUE", requires = "url")]
    pub header: Vec<String>,

    /// Bearer token for Streamable HTTP Authorization.
    #[arg(long, value_name = "TOKEN", requires = "url")]
    pub bearer: Option<String>,

    /// Timeout in seconds for initialization and the requested operation.
    #[arg(long, default_value_t = 30)]
    pub timeout: u64,
}

impl TransportArgs {
    pub fn to_options(&self) -> Result<TransportOptions> {
        let endpoint = match (&self.url, &self.stdio) {
            (Some(url), None) => Endpoint::Http {
                url: url.clone(),
                bearer: self.bearer.clone(),
                headers: parse_key_values(&self.header, "--header")?,
            },
            (None, Some(command)) => Endpoint::Stdio {
                command: command.clone(),
                cwd: self.cwd.clone(),
                env: parse_key_values(&self.env, "--env")?,
            },
            (None, None) => bail!("no MCP transport specified; use --url/--http-url or --stdio"),
            (Some(_), Some(_)) => bail!("use only one MCP transport: --url/--http-url or --stdio"),
        };

        Ok(TransportOptions {
            endpoint,
            timeout_secs: self.timeout,
        })
    }
}

#[derive(Debug, Args)]
pub struct ListArgs {
    #[command(flatten)]
    pub transport: TransportArgs,

    /// Print machine-readable JSON.
    #[arg(long)]
    pub json: bool,

    /// Include full input schemas in text output.
    #[arg(long)]
    pub schema: bool,

    /// Only print tool names.
    #[arg(long)]
    pub brief: bool,
}

#[derive(Debug, Args)]
pub struct CallArgs {
    #[command(flatten)]
    pub transport: TransportArgs,

    /// Tool name, or a simple function-style call such as tool_name(x=1).
    pub target: String,

    /// Tool arguments as a JSON object. Use '-' to read JSON from stdin.
    #[arg(long = "args", value_name = "JSON")]
    pub args_json: Option<String>,

    /// Tool argument in KEY=VALUE or KEY:VALUE form. May be repeated.
    #[arg(long = "arg", value_name = "KEY=VALUE")]
    pub arg: Vec<String>,

    /// Additional KEY=VALUE or KEY:VALUE arguments.
    pub pairs: Vec<String>,

    /// Print the raw MCP CallToolResult JSON.
    #[arg(long)]
    pub json: bool,

    /// Return exit code 0 even when the MCP result has isError=true.
    #[arg(long)]
    pub allow_tool_error: bool,
}

#[derive(Debug, Args)]
pub struct ResourcesArgs {
    #[command(flatten)]
    pub transport: TransportArgs,

    #[command(subcommand)]
    pub command: ResourceCommand,
}

#[derive(Debug, Subcommand)]
pub enum ResourceCommand {
    /// List resources exposed by an MCP server.
    List(ResourceListArgs),
    /// List resource templates exposed by an MCP server.
    Templates(ResourceListArgs),
    /// Read a resource by URI.
    Read(ResourceReadArgs),
}

#[derive(Debug, Args)]
pub struct ResourceListArgs {
    /// Print machine-readable JSON.
    #[arg(long)]
    pub json: bool,

    /// Only print resource URIs or template URI patterns.
    #[arg(long)]
    pub brief: bool,
}

#[derive(Debug, Args)]
pub struct ResourceReadArgs {
    /// Resource URI.
    pub uri: String,

    /// Print the raw MCP ReadResourceResult JSON.
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct PromptsArgs {
    #[command(flatten)]
    pub transport: TransportArgs,

    #[command(subcommand)]
    pub command: PromptCommand,
}

#[derive(Debug, Subcommand)]
pub enum PromptCommand {
    /// List prompts exposed by an MCP server.
    List(PromptListArgs),
    /// Get a prompt by name.
    Get(PromptGetArgs),
}

#[derive(Debug, Args)]
pub struct PromptListArgs {
    /// Print machine-readable JSON.
    #[arg(long)]
    pub json: bool,

    /// Only print prompt names.
    #[arg(long)]
    pub brief: bool,
}

#[derive(Debug, Args)]
pub struct PromptGetArgs {
    /// Prompt name.
    pub name: String,

    /// Prompt arguments as a JSON object. Use '-' to read JSON from stdin.
    #[arg(long = "args", value_name = "JSON")]
    pub args_json: Option<String>,

    /// Prompt argument in KEY=VALUE or KEY:VALUE form. May be repeated.
    #[arg(long = "arg", value_name = "KEY=VALUE")]
    pub arg: Vec<String>,

    /// Additional KEY=VALUE or KEY:VALUE arguments.
    pub pairs: Vec<String>,

    /// Print the raw MCP GetPromptResult JSON.
    #[arg(long)]
    pub json: bool,
}
