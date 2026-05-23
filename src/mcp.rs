use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::{Context, Result, bail};
use http::{HeaderName, HeaderValue};
use rmcp::{
    ServiceExt,
    model::{CallToolRequestParams, CallToolResult, JsonObject, Tool},
    transport::{
        StreamableHttpClientTransport, TokioChildProcess,
        streamable_http_client::StreamableHttpClientTransportConfig,
    },
};
use tokio::process::Command;

use crate::cli::TransportArgs;

enum TransportConfig {
    Http {
        url: String,
        bearer: Option<String>,
        headers: HashMap<HeaderName, HeaderValue>,
    },
    Stdio {
        program: String,
        args: Vec<String>,
        cwd: Option<PathBuf>,
        env: Vec<(String, String)>,
    },
}

pub async fn list_tools(args: &TransportArgs) -> Result<Vec<Tool>> {
    run_with_timeout(args.timeout, "list tools", async {
        match parse_transport(args)? {
            TransportConfig::Http {
                url,
                bearer,
                headers,
            } => {
                let transport = http_transport(url, bearer, headers);
                let mut client = ().serve(transport).await.context("initialize MCP server")?;
                let tools = client
                    .peer()
                    .list_all_tools()
                    .await
                    .context("send tools/list")?;
                let _ = client.close_with_timeout(Duration::from_secs(2)).await;
                Ok(tools)
            }
            TransportConfig::Stdio {
                program,
                args,
                cwd,
                env,
            } => {
                let transport =
                    stdio_transport(program, args, cwd, env).context("spawn stdio MCP server")?;
                let mut client = ().serve(transport).await.context("initialize MCP server")?;
                let tools = client
                    .peer()
                    .list_all_tools()
                    .await
                    .context("send tools/list")?;
                let _ = client.close_with_timeout(Duration::from_secs(2)).await;
                Ok(tools)
            }
        }
    })
    .await
}

pub async fn call_tool(
    args: &TransportArgs,
    tool_name: String,
    arguments: JsonObject,
) -> Result<CallToolResult> {
    run_with_timeout(args.timeout, "call tool", async {
        match parse_transport(args)? {
            TransportConfig::Http {
                url,
                bearer,
                headers,
            } => {
                let transport = http_transport(url, bearer, headers);
                let mut client = ().serve(transport).await.context("initialize MCP server")?;
                let result = client
                    .peer()
                    .call_tool(CallToolRequestParams::new(tool_name).with_arguments(arguments))
                    .await
                    .context("send tools/call")?;
                let _ = client.close_with_timeout(Duration::from_secs(2)).await;
                Ok(result)
            }
            TransportConfig::Stdio {
                program,
                args,
                cwd,
                env,
            } => {
                let transport =
                    stdio_transport(program, args, cwd, env).context("spawn stdio MCP server")?;
                let mut client = ().serve(transport).await.context("initialize MCP server")?;
                let result = client
                    .peer()
                    .call_tool(CallToolRequestParams::new(tool_name).with_arguments(arguments))
                    .await
                    .context("send tools/call")?;
                let _ = client.close_with_timeout(Duration::from_secs(2)).await;
                Ok(result)
            }
        }
    })
    .await
}

async fn run_with_timeout<T>(
    timeout_secs: u64,
    label: &'static str,
    future: impl std::future::Future<Output = Result<T>>,
) -> Result<T> {
    tokio::time::timeout(Duration::from_secs(timeout_secs), future)
        .await
        .with_context(|| format!("{label} timed out after {timeout_secs}s"))?
}

fn parse_transport(args: &TransportArgs) -> Result<TransportConfig> {
    match (&args.url, &args.stdio) {
        (Some(url), None) => Ok(TransportConfig::Http {
            url: url.clone(),
            bearer: args.bearer.clone(),
            headers: parse_headers(&args.header)?,
        }),
        (None, Some(command)) => {
            let mut parts = shell_words::split(command)
                .with_context(|| format!("parse stdio command: {command}"))?;
            if parts.is_empty() {
                bail!("--stdio command cannot be empty");
            }
            let program = parts.remove(0);
            Ok(TransportConfig::Stdio {
                program,
                args: parts,
                cwd: args.cwd.clone(),
                env: parse_key_values(&args.env, "--env")?,
            })
        }
        (None, None) => bail!("no MCP transport specified; use --url/--http-url or --stdio"),
        (Some(_), Some(_)) => bail!("use only one MCP transport: --url/--http-url or --stdio"),
    }
}

fn http_transport(
    url: String,
    bearer: Option<String>,
    headers: HashMap<HeaderName, HeaderValue>,
) -> StreamableHttpClientTransport<reqwest::Client> {
    let mut config = StreamableHttpClientTransportConfig::with_uri(url).custom_headers(headers);
    if let Some(token) = bearer {
        config = config.auth_header(token);
    }
    StreamableHttpClientTransport::from_config(config)
}

fn stdio_transport(
    program: String,
    args: Vec<String>,
    cwd: Option<PathBuf>,
    env: Vec<(String, String)>,
) -> std::io::Result<TokioChildProcess> {
    let mut command = Command::new(resolve_program(&program));
    command.args(args);
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    for (key, value) in env {
        command.env(key, value);
    }
    TokioChildProcess::new(command)
}

fn resolve_program(program: &str) -> String {
    if looks_like_path(program) || Path::new(program).extension().is_some() {
        return program.to_owned();
    }

    #[cfg(windows)]
    {
        if let Some(path) = resolve_windows_path_ext(program) {
            return path;
        }
    }

    program.to_owned()
}

fn looks_like_path(program: &str) -> bool {
    program.contains('/') || program.contains('\\')
}

#[cfg(windows)]
fn resolve_windows_path_ext(program: &str) -> Option<String> {
    let path = std::env::var_os("PATH")?;
    let pathext =
        std::env::var_os("PATHEXT").unwrap_or_else(|| ".COM;.EXE;.BAT;.CMD".to_owned().into());
    let pathext = pathext.to_string_lossy();
    let extensions = pathext
        .split(';')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(str::to_owned)
        .collect::<Vec<_>>();

    for directory in std::env::split_paths(&path) {
        for extension in &extensions {
            let candidate = directory.join(format!("{program}{extension}"));
            if candidate.is_file() {
                return Some(candidate.to_string_lossy().into_owned());
            }
        }
    }

    None
}

fn parse_headers(values: &[String]) -> Result<HashMap<HeaderName, HeaderValue>> {
    let mut headers = HashMap::new();
    for (key, value) in parse_key_values(values, "--header")? {
        let name = HeaderName::from_bytes(key.as_bytes())
            .with_context(|| format!("invalid HTTP header name: {key}"))?;
        let value = HeaderValue::from_str(&value)
            .with_context(|| format!("invalid value for HTTP header {key}"))?;
        headers.insert(name, value);
    }
    Ok(headers)
}

fn parse_key_values(values: &[String], flag: &str) -> Result<Vec<(String, String)>> {
    values
        .iter()
        .map(|value| {
            let (key, raw) = value
                .split_once('=')
                .with_context(|| format!("{flag} expects KEY=VALUE, got {value:?}"))?;
            if key.trim().is_empty() {
                bail!("{flag} key cannot be empty");
            }
            Ok((key.trim().to_owned(), raw.to_owned()))
        })
        .collect()
}
