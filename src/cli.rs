use std::{
    env, fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use clap::{Args, Parser, Subcommand};
use mcpcall_core::{
    ConfigOverlay, Endpoint, McpcallConfig, TransportOptions, parse_key_values, resolve_bearer,
};

#[derive(Debug, Parser)]
#[command(
    name = "mcpcall",
    version,
    about = "Call MCP servers from the command line",
    long_about = "mcpcall is a small Rust CLI for listing and calling tools on any MCP server over stdio, Streamable HTTP, or legacy SSE."
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
    /// Call several tools over one MCP session.
    Batch(BatchArgs),
    /// Inspect connectivity and MCP primitive availability.
    Doctor(DoctorArgs),
    /// List or read resources exposed by an MCP server.
    Resources(ResourcesArgs),
    /// List or get prompts exposed by an MCP server.
    Prompts(PromptsArgs),
    /// Request argument completions from an MCP server.
    Complete(CompleteArgs),
    /// Export generated schemas, types, or shell wrappers.
    Export(ExportArgs),
    /// Discover OAuth metadata or request CI-friendly tokens.
    Auth(AuthArgs),
    /// Manage mcpcall config files.
    Config(ConfigArgs),
}

#[derive(Debug, Args, Clone)]
pub struct TransportArgs {
    /// Named server from a mcpcall or MCP client config.
    #[arg(long, value_name = "NAME", env = "MCP_SERVER")]
    pub server: Option<String>,

    /// Config file containing mcpServers. Defaults to MCPCALL_CONFIG or ~/.config/mcpcall/config.json.
    #[arg(long, value_name = "FILE", env = "MCPCALL_CONFIG")]
    pub config: Option<PathBuf>,

    /// Streamable HTTP MCP endpoint, for example http://127.0.0.1:8765/mcp.
    #[arg(
        long,
        alias = "http-url",
        alias = "mcp-url",
        env = "MCP_URL",
        conflicts_with_all = ["stdio", "sse_url"]
    )]
    pub url: Option<String>,

    /// Legacy SSE MCP endpoint.
    #[arg(long, alias = "sse-url", env = "MCP_SSE_URL", conflicts_with_all = ["url", "stdio"])]
    pub sse_url: Option<String>,

    /// Stdio MCP server command, for example "python -m my_server".
    #[arg(long, value_name = "COMMAND", conflicts_with_all = ["url", "sse_url"])]
    pub stdio: Option<String>,

    /// Working directory for a stdio server command.
    #[arg(long, value_name = "DIR")]
    pub cwd: Option<PathBuf>,

    /// Environment variable for a stdio server, in KEY=VALUE form.
    #[arg(long = "env", value_name = "KEY=VALUE")]
    pub env: Vec<String>,

    /// Extra HTTP header for Streamable HTTP/SSE, in KEY=VALUE form.
    #[arg(long = "header", value_name = "KEY=VALUE")]
    pub header: Vec<String>,

    /// Bearer token for Streamable HTTP/SSE Authorization.
    #[arg(long, value_name = "TOKEN")]
    pub bearer: Option<String>,

    /// Environment variable containing a bearer token for Streamable HTTP/SSE.
    #[arg(long, value_name = "ENV_VAR")]
    pub bearer_env: Option<String>,

    /// Root directory advertised through MCP roots/list.
    #[arg(long, value_name = "DIR")]
    pub root: Vec<PathBuf>,

    /// Timeout in seconds for initialization and the requested operation.
    #[arg(long, default_value_t = 30)]
    pub timeout: u64,
}

impl TransportArgs {
    pub fn to_options(&self) -> Result<TransportOptions> {
        let headers = parse_key_values(&self.header, "--header")?;
        let env_values = parse_key_values(&self.env, "--env")?;
        let roots = self
            .root
            .iter()
            .map(path_to_file_uri)
            .collect::<Result<Vec<_>>>()?;

        let direct_count = usize::from(self.url.is_some())
            + usize::from(self.sse_url.is_some())
            + usize::from(self.stdio.is_some());
        if direct_count > 0 && self.server.is_some() {
            bail!("use either --server or a direct transport flag, not both");
        }

        let endpoint = if let Some(server_name) = &self.server {
            let config_path = resolve_config_path(self.config.as_deref())?;
            let config = load_config(&config_path)?;
            return config
                .server(server_name)?
                .to_transport_options(ConfigOverlay {
                    headers,
                    env: env_values,
                    bearer: self.bearer.clone(),
                    bearer_env: self.bearer_env.clone(),
                    roots,
                    timeout_secs: Some(self.timeout),
                });
        } else if let Some(url) = &self.url {
            if !env_values.is_empty() || self.cwd.is_some() {
                bail!("--env and --cwd apply only to stdio transports");
            }
            Endpoint::Http {
                url: url.clone(),
                bearer: resolve_bearer(self.bearer.as_ref(), self.bearer_env.as_ref())?,
                headers,
            }
        } else if let Some(url) = &self.sse_url {
            if !env_values.is_empty() || self.cwd.is_some() {
                bail!("--env and --cwd apply only to stdio transports");
            }
            Endpoint::Sse {
                url: url.clone(),
                bearer: resolve_bearer(self.bearer.as_ref(), self.bearer_env.as_ref())?,
                headers,
            }
        } else if let Some(command) = &self.stdio {
            if !headers.is_empty() || self.bearer.is_some() || self.bearer_env.is_some() {
                bail!("--header and --bearer apply only to HTTP/SSE transports");
            }
            Endpoint::Stdio {
                command: command.clone(),
                cwd: self.cwd.clone(),
                env: env_values,
            }
        } else {
            bail!(
                "no MCP transport specified; use --server, --url/--http-url, --sse-url, or --stdio"
            );
        };

        Ok(TransportOptions {
            endpoint,
            timeout_secs: self.timeout,
            roots,
        })
    }
}

pub fn resolve_config_path(path: Option<&Path>) -> Result<PathBuf> {
    if let Some(path) = path {
        return Ok(path.to_path_buf());
    }
    if let Ok(path) = env::var("MCPCALL_CONFIG") {
        return Ok(PathBuf::from(path));
    }
    Ok(home_dir()?
        .join(".config")
        .join("mcpcall")
        .join("config.json"))
}

pub fn load_config(path: &Path) -> Result<McpcallConfig> {
    let text = fs::read_to_string(path)
        .with_context(|| format!("read MCP config file {}", path.display()))?;
    McpcallConfig::from_json_str(&text)
}

pub fn write_config(path: &Path, config: &McpcallConfig) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("create config directory {}", parent.display()))?;
    }
    fs::write(path, format!("{}\n", config.to_pretty_json()?))
        .with_context(|| format!("write MCP config file {}", path.display()))
}

fn home_dir() -> Result<PathBuf> {
    if let Some(home) = env::var_os("HOME") {
        return Ok(PathBuf::from(home));
    }
    if let Some(profile) = env::var_os("USERPROFILE") {
        return Ok(PathBuf::from(profile));
    }
    bail!("could not determine home directory; pass --config");
}

fn path_to_file_uri(path: &PathBuf) -> Result<String> {
    let absolute = if path.is_absolute() {
        path.clone()
    } else {
        env::current_dir()
            .context("resolve current directory for --root")?
            .join(path)
    };
    let mut value = absolute.to_string_lossy().replace('\\', "/");
    if cfg!(windows) && value.as_bytes().get(1) == Some(&b':') {
        value = format!("/{value}");
    }
    Ok(format!("file://{}", encode_uri_path(&value)))
}

fn encode_uri_path(value: &str) -> String {
    value
        .chars()
        .flat_map(|ch| match ch {
            ' ' => "%20".chars().collect::<Vec<_>>(),
            '#' => "%23".chars().collect::<Vec<_>>(),
            '?' => "%3F".chars().collect::<Vec<_>>(),
            _ => vec![ch],
        })
        .collect()
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
pub struct BatchArgs {
    #[command(flatten)]
    pub transport: TransportArgs,

    /// JSON file containing an array of {name/tool, arguments} objects. Use '-' or omit to read stdin.
    #[arg(long, value_name = "FILE")]
    pub file: Option<PathBuf>,

    /// Continue running later calls after one call fails at the protocol level.
    #[arg(long)]
    pub continue_on_error: bool,

    /// Print machine-readable JSON.
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct DoctorArgs {
    #[command(flatten)]
    pub transport: TransportArgs,

    /// Print machine-readable JSON.
    #[arg(long)]
    pub json: bool,
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

#[derive(Debug, Args)]
pub struct CompleteArgs {
    #[command(flatten)]
    pub transport: TransportArgs,

    #[command(subcommand)]
    pub command: CompleteCommand,
}

#[derive(Debug, Subcommand)]
pub enum CompleteCommand {
    /// Complete a prompt argument.
    Prompt(CompletePromptArgs),
    /// Complete a resource template argument.
    Resource(CompleteResourceArgs),
}

#[derive(Debug, Args)]
pub struct CompletePromptArgs {
    pub prompt: String,
    pub argument: String,
    pub value: String,

    /// Completion context argument in KEY=VALUE or KEY:VALUE form. May be repeated.
    #[arg(long = "context", value_name = "KEY=VALUE")]
    pub context: Vec<String>,

    /// Print the raw MCP completion JSON.
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct CompleteResourceArgs {
    pub uri_template: String,
    pub argument: String,
    pub value: String,

    /// Completion context argument in KEY=VALUE or KEY:VALUE form. May be repeated.
    #[arg(long = "context", value_name = "KEY=VALUE")]
    pub context: Vec<String>,

    /// Print the raw MCP completion JSON.
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct ExportArgs {
    #[command(flatten)]
    pub transport: TransportArgs,

    #[command(subcommand)]
    pub command: ExportCommand,
}

#[derive(Debug, Subcommand)]
pub enum ExportCommand {
    /// Generate TypeScript declarations for tool names and arguments.
    Types(ExportTypesArgs),
    /// Generate shell wrapper functions for each tool.
    Shell(ExportShellArgs),
}

#[derive(Debug, Args)]
pub struct ExportTypesArgs {
    /// TypeScript namespace prefix.
    #[arg(long, default_value = "Mcpcall")]
    pub namespace: String,
}

#[derive(Debug, Args)]
pub struct ExportShellArgs {
    /// Shell dialect to emit.
    #[arg(long, value_enum, default_value_t = ShellDialect::Powershell)]
    pub shell: ShellDialect,
}

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum ShellDialect {
    Bash,
    Powershell,
}

#[derive(Debug, Args)]
pub struct AuthArgs {
    #[command(subcommand)]
    pub command: AuthCommand,
}

#[derive(Debug, Subcommand)]
pub enum AuthCommand {
    /// Discover OAuth metadata for an MCP URL origin.
    Discover(AuthDiscoverArgs),
    /// Request an OAuth2 client-credentials access token.
    ClientCredentials(AuthClientCredentialsArgs),
}

#[derive(Debug, Args)]
pub struct AuthDiscoverArgs {
    /// MCP resource URL, for example https://example.com/mcp.
    #[arg(long, alias = "mcp-url", env = "MCP_URL")]
    pub url: String,

    /// Print machine-readable JSON.
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct AuthClientCredentialsArgs {
    /// OAuth token endpoint.
    #[arg(long)]
    pub token_url: String,

    /// OAuth client id.
    #[arg(long)]
    pub client_id: String,

    /// Environment variable containing the OAuth client secret.
    #[arg(long)]
    pub client_secret_env: String,

    /// OAuth scope. May be repeated.
    #[arg(long)]
    pub scope: Vec<String>,

    /// Print the whole token response instead of only access_token.
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct ConfigArgs {
    /// Config file to read or write.
    #[arg(long, value_name = "FILE", env = "MCPCALL_CONFIG")]
    pub config: Option<PathBuf>,

    #[command(subcommand)]
    pub command: ConfigCommand,
}

#[derive(Debug, Subcommand)]
pub enum ConfigCommand {
    /// Print the config path mcpcall will use.
    Path,
    /// List configured MCP servers.
    List(ConfigListArgs),
    /// Show one configured MCP server.
    Show(ConfigShowArgs),
    /// Import and normalize an MCP client config file.
    Import(ConfigImportArgs),
    /// Add or replace one server in a mcpcall config file.
    Add(Box<ConfigAddArgs>),
}

#[derive(Debug, Args)]
pub struct ConfigListArgs {
    /// Print machine-readable JSON.
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct ConfigShowArgs {
    pub name: String,

    /// Print machine-readable JSON.
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct ConfigImportArgs {
    /// Source config file.
    #[arg(long, value_name = "FILE")]
    pub from: PathBuf,

    /// Output config file. Omit to print the normalized config.
    #[arg(long, value_name = "FILE")]
    pub output: Option<PathBuf>,

    /// Merge imported servers into the output config instead of replacing it.
    #[arg(long)]
    pub merge: bool,

    /// Print machine-readable JSON after import.
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct ConfigAddArgs {
    pub name: String,

    /// Streamable HTTP endpoint URL.
    #[arg(long, conflicts_with_all = ["stdio", "sse_url"])]
    pub url: Option<String>,

    /// Legacy SSE endpoint URL.
    #[arg(long, alias = "sse-url", conflicts_with_all = ["stdio", "url"])]
    pub sse_url: Option<String>,

    /// Stdio command.
    #[arg(long, value_name = "COMMAND", conflicts_with_all = ["url", "sse_url"])]
    pub stdio: Option<String>,

    /// Stdio command argument. May be repeated.
    #[arg(long = "stdio-arg", value_name = "ARG")]
    pub stdio_arg: Vec<String>,

    /// Working directory for stdio.
    #[arg(long, value_name = "DIR")]
    pub cwd: Option<PathBuf>,

    /// Environment variable for stdio, in KEY=VALUE form.
    #[arg(long = "env", value_name = "KEY=VALUE")]
    pub env: Vec<String>,

    /// HTTP/SSE header, in KEY=VALUE form.
    #[arg(long = "header", value_name = "KEY=VALUE")]
    pub header: Vec<String>,

    /// Bearer token for HTTP/SSE.
    #[arg(long)]
    pub bearer: Option<String>,

    /// Environment variable containing a bearer token.
    #[arg(long, value_name = "ENV_VAR")]
    pub bearer_env: Option<String>,

    /// Root URI or directory for roots/list.
    #[arg(long, value_name = "ROOT")]
    pub root: Vec<String>,
}
