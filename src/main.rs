use std::process::ExitCode;

use anyhow::Result;
use clap::Parser;
use mcpcall_core::{
    BatchToolCall, ConfigServer, DiscoveredConfig, McpcallConfig, ToolInfo, output,
};
use serde::Serialize;
use serde_json::{Value, json};
use std::{collections::BTreeMap, env, io::Read, path::Path, time::Duration};

mod cli;

use cli::{
    AuthCommand, Cli, Command, CompleteCommand, ConfigCommand, ExportCommand, PromptCommand,
    ResourceCommand, ShellDialect, load_config, resolve_config_path, write_config,
};

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(code) => code,
        Err(error) => {
            eprintln!("error: {error:#}");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> Result<ExitCode> {
    let cli = Cli::parse();
    init_logging(cli.verbose);

    match cli.command {
        Command::List(args) => {
            let options = args.transport.to_options()?;
            let tools = mcpcall_rmcp::list_tools(&options).await?;
            output::print_tools(&tools, args.json, args.schema, args.brief)?;
            Ok(ExitCode::SUCCESS)
        }
        Command::Call(args) => {
            let options = args.transport.to_options()?;
            let parsed = mcpcall_core::parse_call_arguments(
                &args.target,
                args.args_json.as_deref(),
                &args.arg,
                &args.pairs,
            )?;
            let result = mcpcall_rmcp::call_tool(&options, parsed.name, parsed.arguments).await?;
            output::print_call_result(&result, args.json)?;
            if result.is_error && !args.allow_tool_error {
                Ok(ExitCode::from(2))
            } else {
                Ok(ExitCode::SUCCESS)
            }
        }
        Command::Batch(args) => {
            let options = args.transport.to_options()?;
            let calls = read_batch_tool_calls(args.file.as_deref())?;
            let results =
                mcpcall_rmcp::call_tool_batch(&options, calls, args.continue_on_error).await?;
            output::print_batch_results(&results, args.json)?;
            if results.iter().any(|item| !item.ok) {
                Ok(ExitCode::from(2))
            } else {
                Ok(ExitCode::SUCCESS)
            }
        }
        Command::Doctor(args) => {
            let options = args.transport.to_options()?;
            let report = mcpcall_rmcp::inspect_server(&options).await?;
            output::print_doctor_report(&report, args.json)?;
            Ok(ExitCode::SUCCESS)
        }
        Command::Resources(args) => {
            let options = args.transport.to_options()?;
            match args.command {
                ResourceCommand::List(list_args) => {
                    let resources = mcpcall_rmcp::list_resources(&options).await?;
                    output::print_resources(&resources, list_args.json, list_args.brief)?;
                }
                ResourceCommand::Templates(list_args) => {
                    let templates = mcpcall_rmcp::list_resource_templates(&options).await?;
                    output::print_resource_templates(&templates, list_args.json, list_args.brief)?;
                }
                ResourceCommand::Read(read_args) => {
                    let result = mcpcall_rmcp::read_resource(&options, read_args.uri).await?;
                    output::print_read_resource(&result, read_args.json)?;
                }
            }
            Ok(ExitCode::SUCCESS)
        }
        Command::Prompts(args) => {
            let options = args.transport.to_options()?;
            match args.command {
                PromptCommand::List(list_args) => {
                    let prompts = mcpcall_rmcp::list_prompts(&options).await?;
                    output::print_prompts(&prompts, list_args.json, list_args.brief)?;
                }
                PromptCommand::Get(get_args) => {
                    let parsed = mcpcall_core::parse_named_arguments(
                        &get_args.name,
                        get_args.args_json.as_deref(),
                        &get_args.arg,
                        &get_args.pairs,
                    )?;
                    let result =
                        mcpcall_rmcp::get_prompt(&options, parsed.name, parsed.arguments).await?;
                    output::print_prompt_result(&result, get_args.json)?;
                }
            }
            Ok(ExitCode::SUCCESS)
        }
        Command::Complete(args) => {
            let options = args.transport.to_options()?;
            match args.command {
                CompleteCommand::Prompt(prompt_args) => {
                    let context = mcpcall_core::parse_named_arguments(
                        "context",
                        None,
                        &prompt_args.context,
                        &[],
                    )?
                    .arguments;
                    let result = mcpcall_rmcp::complete_prompt(
                        &options,
                        prompt_args.prompt,
                        prompt_args.argument,
                        prompt_args.value,
                        context,
                    )
                    .await?;
                    output::print_completion_result(&result, prompt_args.json)?;
                }
                CompleteCommand::Resource(resource_args) => {
                    let context = mcpcall_core::parse_named_arguments(
                        "context",
                        None,
                        &resource_args.context,
                        &[],
                    )?
                    .arguments;
                    let result = mcpcall_rmcp::complete_resource(
                        &options,
                        resource_args.uri_template,
                        resource_args.argument,
                        resource_args.value,
                        context,
                    )
                    .await?;
                    output::print_completion_result(&result, resource_args.json)?;
                }
            }
            Ok(ExitCode::SUCCESS)
        }
        Command::Export(args) => {
            let options = args.transport.to_options()?;
            let tools = mcpcall_rmcp::list_tools(&options).await?;
            match args.command {
                ExportCommand::Types(type_args) => {
                    print_typescript_types(&type_args.namespace, &tools)?;
                }
                ExportCommand::Shell(shell_args) => {
                    print_shell_wrappers(shell_args.shell, &args.transport, &tools);
                }
            }
            Ok(ExitCode::SUCCESS)
        }
        Command::Config(args) => {
            handle_config(args)?;
            Ok(ExitCode::SUCCESS)
        }
        Command::Auth(args) => {
            handle_auth(args).await?;
            Ok(ExitCode::SUCCESS)
        }
    }
}

async fn handle_auth(args: cli::AuthArgs) -> Result<()> {
    match args.command {
        AuthCommand::Discover(discover_args) => {
            let report = discover_oauth_metadata(&discover_args.url).await?;
            if discover_args.json {
                output::print_json_value(&report)?;
            } else {
                print_auth_discovery(&report);
            }
        }
        AuthCommand::ClientCredentials(token_args) => {
            let response = request_client_credentials_token(&token_args).await?;
            if token_args.json {
                output::print_json_value(&response)?;
            } else if let Some(token) = response.get("access_token").and_then(Value::as_str) {
                println!("{token}");
            } else {
                output::print_json_value(&response)?;
            }
        }
    }
    Ok(())
}

async fn discover_oauth_metadata(url: &str) -> Result<Value> {
    let target = reqwest::Url::parse(url)?;
    let host = target
        .host_str()
        .ok_or_else(|| anyhow::anyhow!("OAuth discovery URL must include a host"))?;
    let mut origin = format!("{}://{}", target.scheme(), host);
    if let Some(port) = target.port() {
        origin.push_str(&format!(":{port}"));
    }

    let protected_resource_url = format!("{origin}/.well-known/oauth-protected-resource");
    let authorization_server_url = format!("{origin}/.well-known/oauth-authorization-server");
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .build()?;
    let protected_resource =
        fetch_optional_json(&client, "protected_resource", &protected_resource_url).await;
    let authorization_server =
        fetch_optional_json(&client, "authorization_server", &authorization_server_url).await;

    Ok(json!({
        "target": url,
        "origin": origin,
        "protected_resource_metadata_url": protected_resource_url,
        "authorization_server_metadata_url": authorization_server_url,
        "protected_resource": protected_resource,
        "authorization_server": authorization_server,
    }))
}

async fn fetch_optional_json(client: &reqwest::Client, kind: &str, url: &str) -> Value {
    let response = match client
        .get(url)
        .header(reqwest::header::ACCEPT, "application/json")
        .send()
        .await
    {
        Ok(response) => response,
        Err(error) => {
            return json!({
                "kind": kind,
                "ok": false,
                "url": url,
                "error": error.to_string(),
            });
        }
    };
    let status = response.status();
    let body = match response.text().await {
        Ok(body) => body,
        Err(error) => {
            return json!({
                "kind": kind,
                "ok": false,
                "url": url,
                "status": status.as_u16(),
                "error": error.to_string(),
            });
        }
    };
    if !status.is_success() {
        return json!({
            "kind": kind,
            "ok": false,
            "url": url,
            "status": status.as_u16(),
        });
    }
    match serde_json::from_str::<Value>(&body) {
        Ok(metadata) => json!({
            "kind": kind,
            "ok": true,
            "url": url,
            "status": status.as_u16(),
            "metadata": metadata,
        }),
        Err(error) => json!({
            "kind": kind,
            "ok": false,
            "url": url,
            "status": status.as_u16(),
            "error": format!("invalid JSON metadata: {error}"),
        }),
    }
}

async fn request_client_credentials_token(args: &cli::AuthClientCredentialsArgs) -> Result<Value> {
    let client_secret = env::var(&args.client_secret_env)
        .map_err(|_| anyhow::anyhow!("{} is not set", args.client_secret_env))?;
    let mut form = vec![
        ("grant_type", "client_credentials".to_owned()),
        ("client_id", args.client_id.clone()),
        ("client_secret", client_secret),
    ];
    if !args.scope.is_empty() {
        form.push(("scope", args.scope.join(" ")));
    }

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()?;
    let response = client.post(&args.token_url).form(&form).send().await?;
    let status = response.status();
    let body = response.text().await?;
    if !status.is_success() {
        anyhow::bail!(
            "token endpoint returned HTTP {}: {}",
            status.as_u16(),
            body.trim()
        );
    }
    serde_json::from_str(&body).map_err(Into::into)
}

fn print_auth_discovery(report: &Value) {
    println!(
        "target: {}",
        report.get("target").and_then(Value::as_str).unwrap_or("")
    );
    println!(
        "origin: {}",
        report.get("origin").and_then(Value::as_str).unwrap_or("")
    );
    print_auth_discovery_item("protected resource", &report["protected_resource"]);
    print_auth_discovery_item("authorization server", &report["authorization_server"]);
}

fn print_auth_discovery_item(label: &str, item: &Value) {
    let url = item.get("url").and_then(Value::as_str).unwrap_or("");
    if item.get("ok").and_then(Value::as_bool).unwrap_or(false) {
        println!("{label}: ok ({url})");
        if let Some(issuer) = item
            .get("metadata")
            .and_then(|metadata| metadata.get("issuer"))
            .and_then(Value::as_str)
        {
            println!("  issuer: {issuer}");
        }
        if let Some(token_endpoint) = item
            .get("metadata")
            .and_then(|metadata| metadata.get("token_endpoint"))
            .and_then(Value::as_str)
        {
            println!("  token endpoint: {token_endpoint}");
        }
    } else if let Some(status) = item.get("status").and_then(Value::as_u64) {
        println!("{label}: HTTP {status} ({url})");
    } else if let Some(error) = item.get("error").and_then(Value::as_str) {
        println!("{label}: {error} ({url})");
    } else {
        println!("{label}: unavailable ({url})");
    }
}

fn read_batch_tool_calls(path: Option<&Path>) -> Result<Vec<BatchToolCall>> {
    let mut input = String::new();
    match path {
        Some(path) if path != Path::new("-") => {
            input = std::fs::read_to_string(path)?;
        }
        _ => {
            std::io::stdin().read_to_string(&mut input)?;
        }
    }
    serde_json::from_str(&input).map_err(Into::into)
}

fn handle_config(args: cli::ConfigArgs) -> Result<()> {
    let path = resolve_config_path(args.config.as_deref())?;
    match args.command {
        ConfigCommand::Path => {
            println!("{}", path.display());
        }
        ConfigCommand::List(list_args) => {
            let config = load_config(&path)?;
            if list_args.json {
                output::print_json_value(&config)?;
            } else if config.mcp_servers.is_empty() {
                println!("No servers configured.");
            } else {
                for name in config.server_names() {
                    println!("{name}");
                }
            }
        }
        ConfigCommand::Show(show_args) => {
            let config = load_config(&path)?;
            let server = config.server(&show_args.name)?;
            if show_args.json {
                output::print_json_value(server)?;
            } else {
                print_config_server(&show_args.name, server);
            }
        }
        ConfigCommand::Import(import_args) => {
            let mut imported = load_config(&import_args.from)?;
            if let Some(output_path) = import_args.output.as_deref() {
                if import_args.merge && output_path.exists() {
                    let mut existing = load_config(output_path)?;
                    existing.mcp_servers.append(&mut imported.mcp_servers);
                    imported = existing;
                }
                write_config(output_path, &imported)?;
                if import_args.json {
                    output::print_json_value(&imported)?;
                } else {
                    println!(
                        "imported {} server(s) into {}",
                        imported.mcp_servers.len(),
                        output_path.display()
                    );
                }
            } else if import_args.json {
                output::print_json_value(&imported)?;
            } else {
                println!("{}", imported.to_pretty_json()?);
            }
        }
        ConfigCommand::Discover(discover_args) => {
            handle_config_discover(&path, discover_args)?;
        }
        ConfigCommand::Add(add_args) => {
            let mut config = if path.exists() {
                load_config(&path)?
            } else {
                McpcallConfig::default()
            };
            let server = config_server_from_add_args(&add_args)?;
            config.mcp_servers.insert(add_args.name.clone(), server);
            write_config(&path, &config)?;
            println!("saved {} in {}", add_args.name, path.display());
        }
    }
    Ok(())
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ConfigDiscoveryReport {
    root: std::path::PathBuf,
    source_count: usize,
    server_count: usize,
    sources: Vec<ConfigDiscoverySourceReport>,
    config: McpcallConfig,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ConfigDiscoverySourceReport {
    kind: String,
    path: std::path::PathBuf,
    server_count: usize,
    servers: Vec<String>,
}

fn handle_config_discover(default_output_path: &Path, args: cli::ConfigDiscoverArgs) -> Result<()> {
    let root = match args.root {
        Some(root) => root,
        None => env::current_dir()?,
    };
    let discovered = mcpcall_core::discover_configs(&root)
        .into_iter()
        .collect::<Result<Vec<_>>>()?;
    let mut config = mcpcall_core::merge_discovered_configs(&discovered);

    if let Some(output_path) = args.output.as_deref() {
        if args.merge && output_path.exists() {
            let mut existing = load_config(output_path)?;
            for (name, server) in config.mcp_servers {
                existing.mcp_servers.entry(name).or_insert(server);
            }
            config = existing;
        }
        write_config(output_path, &config)?;
        if args.json {
            output::print_json_value(&config)?;
        } else {
            println!(
                "imported {} server(s) from {} config source(s) into {}",
                config.mcp_servers.len(),
                discovered.len(),
                output_path.display()
            );
        }
        return Ok(());
    }

    let report = config_discovery_report(root, discovered, config);
    if args.json {
        output::print_json_value(&report)?;
    } else {
        print_config_discovery_report(&report, default_output_path);
    }
    Ok(())
}

fn config_discovery_report(
    root: std::path::PathBuf,
    discovered: Vec<DiscoveredConfig>,
    config: McpcallConfig,
) -> ConfigDiscoveryReport {
    let sources = discovered
        .into_iter()
        .map(|source| {
            let servers = source.config.server_names();
            ConfigDiscoverySourceReport {
                kind: source.kind,
                path: source.path,
                server_count: servers.len(),
                servers: servers.into_iter().map(str::to_owned).collect(),
            }
        })
        .collect::<Vec<_>>();

    ConfigDiscoveryReport {
        root,
        source_count: sources.len(),
        server_count: config.mcp_servers.len(),
        sources,
        config,
    }
}

fn print_config_discovery_report(report: &ConfigDiscoveryReport, default_output_path: &Path) {
    if report.sources.is_empty() {
        println!("No MCP config files found under common client locations.");
        println!(
            "Pass --root DIR to scan another project, or use --output {} to write discovered servers.",
            default_output_path.display()
        );
        return;
    }

    println!(
        "Found {} server(s) in {} config source(s).",
        report.server_count, report.source_count
    );
    for source in &report.sources {
        let servers = if source.servers.is_empty() {
            "no servers".to_owned()
        } else {
            source.servers.join(", ")
        };
        println!("{}: {} ({})", source.kind, source.path.display(), servers);
    }
    println!(
        "Use --output {} to write a merged mcpcall config.",
        default_output_path.display()
    );
}

fn config_server_from_add_args(args: &cli::ConfigAddArgs) -> Result<ConfigServer> {
    let direct_count = usize::from(args.url.is_some())
        + usize::from(args.sse_url.is_some())
        + usize::from(args.stdio.is_some());
    if direct_count != 1 {
        anyhow::bail!("config add requires exactly one of --url, --sse-url, or --stdio");
    }

    let env = key_values_to_map(mcpcall_core::parse_key_values(&args.env, "--env")?);
    let headers = key_values_to_map(mcpcall_core::parse_key_values(&args.header, "--header")?);
    if args.stdio.is_some()
        && (!headers.is_empty() || args.bearer.is_some() || args.bearer_env.is_some())
    {
        anyhow::bail!("--header and --bearer apply only to HTTP/SSE config entries");
    }
    if args.stdio.is_none() && (!env.is_empty() || args.cwd.is_some() || !args.stdio_arg.is_empty())
    {
        anyhow::bail!("--env, --cwd, and --stdio-arg apply only to stdio config entries");
    }

    Ok(ConfigServer {
        command: args.stdio.clone(),
        args: args.stdio_arg.clone(),
        cwd: args.cwd.clone(),
        env,
        url: args.url.clone(),
        sse_url: args.sse_url.clone(),
        headers,
        bearer: args.bearer.clone(),
        bearer_env: args.bearer_env.clone(),
        roots: args.root.clone(),
        ..ConfigServer::default()
    })
}

fn key_values_to_map(values: Vec<mcpcall_core::KeyValue>) -> BTreeMap<String, String> {
    values
        .into_iter()
        .map(|item| (item.key, item.value))
        .collect()
}

fn print_config_server(name: &str, server: &ConfigServer) {
    println!("{name}");
    if let Some(url) = &server.url {
        println!("  url: {url}");
    }
    if let Some(url) = &server.sse_url {
        println!("  sse: {url}");
    }
    if let Some(command) = &server.command {
        println!("  command: {command}");
    }
    if !server.args.is_empty() {
        println!("  args: {}", server.args.join(" "));
    }
    if !server.roots.is_empty() {
        println!("  roots: {}", server.roots.join(", "));
    }
}

fn print_typescript_types(namespace: &str, tools: &[ToolInfo]) -> Result<()> {
    println!(
        "export type {namespace}ToolName = {};",
        ts_tool_name_union(tools)
    );
    println!();
    println!("export interface {namespace}ToolArguments {{");
    for tool in tools {
        println!(
            "  {}: {};",
            serde_json::to_string(&tool.name)?,
            ts_type_for_schema(&tool.input_schema)
        );
    }
    println!("}}");
    println!();
    println!("export interface {namespace}ToolResults {{");
    for tool in tools {
        let result_type = tool
            .raw
            .get("outputSchema")
            .map(ts_type_for_schema)
            .unwrap_or_else(|| "unknown".to_owned());
        println!("  {}: {result_type};", serde_json::to_string(&tool.name)?);
    }
    println!("}}");
    println!();
    println!("export const {namespace}Tools = [");
    for tool in tools {
        println!("  {},", serde_json::to_string(&tool.name)?);
    }
    println!("] as const;");
    Ok(())
}

fn ts_tool_name_union(tools: &[ToolInfo]) -> String {
    if tools.is_empty() {
        return "never".to_owned();
    }
    tools
        .iter()
        .map(|tool| serde_json::to_string(&tool.name).unwrap_or_else(|_| "\"\"".to_owned()))
        .collect::<Vec<_>>()
        .join(" | ")
}

fn ts_type_for_schema(schema: &Value) -> String {
    if let Some(values) = schema.get("enum").and_then(Value::as_array) {
        let variants = values
            .iter()
            .filter_map(|value| serde_json::to_string(value).ok())
            .collect::<Vec<_>>();
        if !variants.is_empty() {
            return variants.join(" | ");
        }
    }
    if let Some(items) = schema.get("anyOf").and_then(Value::as_array) {
        return ts_union(items);
    }
    if let Some(items) = schema.get("oneOf").and_then(Value::as_array) {
        return ts_union(items);
    }
    match schema.get("type").and_then(Value::as_str) {
        Some("object") => ts_object(schema),
        Some("array") => format!(
            "{}[]",
            schema
                .get("items")
                .map(ts_type_for_schema)
                .unwrap_or_else(|| "unknown".to_owned())
        ),
        Some("string") => "string".to_owned(),
        Some("integer" | "number") => "number".to_owned(),
        Some("boolean") => "boolean".to_owned(),
        Some("null") => "null".to_owned(),
        _ => "unknown".to_owned(),
    }
}

fn ts_union(items: &[Value]) -> String {
    let values = items.iter().map(ts_type_for_schema).collect::<Vec<_>>();
    if values.is_empty() {
        "unknown".to_owned()
    } else {
        values.join(" | ")
    }
}

fn ts_object(schema: &Value) -> String {
    let Some(properties) = schema.get("properties").and_then(Value::as_object) else {
        return "Record<string, unknown>".to_owned();
    };
    if properties.is_empty() {
        return "Record<string, never>".to_owned();
    }
    let required = schema
        .get("required")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .collect::<std::collections::BTreeSet<_>>()
        })
        .unwrap_or_default();
    let fields = properties
        .iter()
        .map(|(name, property)| {
            let optional = if required.contains(name.as_str()) {
                ""
            } else {
                "?"
            };
            format!(
                "{}{optional}: {}",
                serde_json::to_string(name).unwrap_or_else(|_| "\"\"".to_owned()),
                ts_type_for_schema(property)
            )
        })
        .collect::<Vec<_>>();
    format!("{{ {} }}", fields.join("; "))
}

fn print_shell_wrappers(dialect: ShellDialect, transport: &cli::TransportArgs, tools: &[ToolInfo]) {
    match dialect {
        ShellDialect::Bash => {
            println!("# generated by mcpcall export shell --shell bash");
            for tool in tools {
                let function_name = shell_safe_name(&tool.name);
                println!(
                    "{function_name}() {{ mcpcall call {} {} \"$@\"; }}",
                    transport_args_for_wrapper(transport, dialect),
                    shell_quote(&tool.name)
                );
            }
        }
        ShellDialect::Powershell => {
            println!("# generated by mcpcall export shell --shell powershell");
            for tool in tools {
                let function_name = shell_safe_name(&tool.name);
                println!("function {function_name} {{");
                println!(
                    "  mcpcall call {} {} @args",
                    transport_args_for_wrapper(transport, dialect),
                    powershell_quote(&tool.name)
                );
                println!("}}");
            }
        }
    }
}

fn transport_args_for_wrapper(transport: &cli::TransportArgs, dialect: ShellDialect) -> String {
    let quote = |value: &str| match dialect {
        ShellDialect::Bash => shell_quote(value),
        ShellDialect::Powershell => powershell_quote(value),
    };
    if let Some(server) = &transport.server {
        return format!("--server {}", quote(server));
    }
    if let Some(url) = &transport.url {
        return format!("--url {}", quote(url));
    }
    if let Some(url) = &transport.sse_url {
        return format!("--sse-url {}", quote(url));
    }
    if let Some(command) = &transport.stdio {
        return format!("--stdio {}", quote(command));
    }
    String::new()
}

fn shell_safe_name(name: &str) -> String {
    let mut value = name
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();
    if value.is_empty() || value.as_bytes()[0].is_ascii_digit() {
        value.insert_str(0, "mcp_");
    }
    value
}

fn powershell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn shell_quote(value: &str) -> String {
    if value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || "-_./:=".contains(ch))
    {
        value.to_owned()
    } else {
        format!("'{}'", value.replace('\'', r#"'\''"#))
    }
}

fn init_logging(verbose: u8) {
    if verbose == 0 {
        return;
    }
    let directive = if verbose == 1 { "info" } else { "debug" };
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| directive.into()),
        )
        .with_writer(std::io::stderr)
        .try_init();
}
