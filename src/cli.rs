use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

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
