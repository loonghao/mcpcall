use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::{Context, Result, bail};
use futures::{StreamExt, stream::BoxStream};
use http::{HeaderName, HeaderValue};
use mcpcall_core::{
    BatchToolCall, BatchToolOutput, CallOutput, CompletionOutput, ContentBlock, DoctorReport,
    Endpoint, KeyValue, PrimitiveProbe, PromptArgumentInfo, PromptInfo, PromptOutput,
    ReadResourceOutput, ResourceContent, ResourceInfo, ResourceTemplateInfo, ToolInfo,
    TransportOptions,
};
use rmcp::{
    ClientHandler, ServiceExt,
    model::{
        CallToolRequestParams, CallToolResult, ClientCapabilities, ClientInfo, CompletionContext,
        CompletionInfo, Content, GetPromptRequestParams, GetPromptResult, Implementation,
        JsonObject, ListRootsResult, LoggingMessageNotificationParam, ProgressNotificationParam,
        Prompt, RawContent, ReadResourceRequestParams, ReadResourceResult, Resource,
        ResourceContents, ResourceTemplate, Root, RootsCapabilities, ServerJsonRpcMessage, Tool,
    },
    service::{NotificationContext, Peer, RoleClient},
    transport::{
        StreamableHttpClientTransport, TokioChildProcess,
        streamable_http_client::StreamableHttpClientTransportConfig,
    },
};
use serde::Serialize;
use serde_json::Value;
use sse_stream::{Sse, SseStream};
use tokio::process::Command;

enum TransportConfig {
    Http {
        url: String,
        bearer: Option<String>,
        headers: HashMap<HeaderName, HeaderValue>,
    },
    Sse {
        url: String,
        bearer: Option<String>,
        headers: HashMap<HeaderName, HeaderValue>,
    },
    Stdio {
        program: String,
        args: Vec<String>,
        cwd: Option<PathBuf>,
        env: Vec<KeyValue>,
    },
}

#[derive(Debug, Clone)]
struct McpcallClient {
    roots: Vec<Root>,
}

#[derive(Debug)]
struct LegacySseError {
    message: String,
}

impl LegacySseError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for LegacySseError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for LegacySseError {}

impl From<reqwest::Error> for LegacySseError {
    fn from(error: reqwest::Error) -> Self {
        Self::new(format!("HTTP error: {error}"))
    }
}

impl From<serde_json::Error> for LegacySseError {
    fn from(error: serde_json::Error) -> Self {
        Self::new(format!("JSON-RPC decode error: {error}"))
    }
}

impl From<sse_stream::Error> for LegacySseError {
    fn from(error: sse_stream::Error) -> Self {
        Self::new(format!("SSE error: {error}"))
    }
}

impl From<http::uri::InvalidUri> for LegacySseError {
    fn from(error: http::uri::InvalidUri) -> Self {
        Self::new(format!("invalid SSE endpoint URI: {error}"))
    }
}

impl From<http::uri::InvalidUriParts> for LegacySseError {
    fn from(error: http::uri::InvalidUriParts) -> Self {
        Self::new(format!("invalid SSE endpoint URI parts: {error}"))
    }
}

type LegacySseEventStream = BoxStream<'static, std::result::Result<Sse, sse_stream::Error>>;
type LegacySseMessageStream =
    BoxStream<'static, std::result::Result<ServerJsonRpcMessage, LegacySseError>>;

struct LegacySseTransport {
    client: reqwest::Client,
    message_endpoint: String,
    bearer: Option<String>,
    headers: HashMap<HeaderName, HeaderValue>,
    stream: Option<LegacySseMessageStream>,
}

impl rmcp::transport::Transport<RoleClient> for LegacySseTransport {
    type Error = LegacySseError;

    fn send(
        &mut self,
        item: rmcp::service::TxJsonRpcMessage<RoleClient>,
    ) -> impl Future<Output = std::result::Result<(), Self::Error>> + Send + 'static {
        let client = self.client.clone();
        let url = self.message_endpoint.clone();
        let bearer = self.bearer.clone();
        let headers = self.headers.clone();
        async move {
            let request = apply_reqwest_headers(client.post(url).json(&item), &headers, bearer);
            request
                .send()
                .await?
                .error_for_status()
                .map(drop)
                .map_err(LegacySseError::from)
        }
    }

    async fn receive(&mut self) -> Option<ServerJsonRpcMessage> {
        self.stream.as_mut()?.next().await?.ok()
    }

    async fn close(&mut self) -> std::result::Result<(), Self::Error> {
        self.stream.take();
        Ok(())
    }
}

impl LegacySseTransport {
    async fn start(
        url: String,
        bearer: Option<String>,
        headers: HashMap<HeaderName, HeaderValue>,
    ) -> std::result::Result<Self, LegacySseError> {
        let client = reqwest::Client::default();
        let mut event_stream =
            open_legacy_sse_stream(&client, &url, &headers, bearer.clone()).await?;
        let message_endpoint = wait_for_legacy_message_endpoint(&url, &mut event_stream).await?;
        let stream = event_stream
            .filter_map(|event| async move {
                match event {
                    Ok(sse) => decode_legacy_sse_message(sse),
                    Err(error) => Some(Err(LegacySseError::from(error))),
                }
            })
            .boxed();

        Ok(Self {
            client,
            message_endpoint,
            bearer,
            headers,
            stream: Some(stream),
        })
    }
}

impl McpcallClient {
    fn from_options(options: &TransportOptions) -> Self {
        Self {
            roots: options
                .roots
                .iter()
                .map(|uri| Root::new(uri.clone()))
                .collect(),
        }
    }
}

impl ClientHandler for McpcallClient {
    async fn list_roots(
        &self,
        _context: rmcp::service::RequestContext<RoleClient>,
    ) -> Result<ListRootsResult, rmcp::model::ErrorData> {
        Ok(ListRootsResult::new(self.roots.clone()))
    }

    async fn on_progress(
        &self,
        params: ProgressNotificationParam,
        _context: NotificationContext<RoleClient>,
    ) {
        tracing::info!(?params, "mcp progress");
    }

    async fn on_logging_message(
        &self,
        params: LoggingMessageNotificationParam,
        _context: NotificationContext<RoleClient>,
    ) {
        tracing::info!(?params, "mcp log");
    }

    fn get_info(&self) -> ClientInfo {
        let mut info = ClientInfo::default();
        info.client_info = Implementation::new("mcpcall", env!("CARGO_PKG_VERSION"));
        if !self.roots.is_empty() {
            let mut capabilities = ClientCapabilities::default();
            capabilities.roots = Some(RootsCapabilities {
                list_changed: Some(false),
            });
            info.capabilities = capabilities;
        }
        info
    }
}

pub async fn list_tools(options: &TransportOptions) -> Result<Vec<ToolInfo>> {
    run_with_peer(options, "list tools", |peer| async move {
        let tools = peer.list_all_tools().await.context("send tools/list")?;
        tools.into_iter().map(convert_tool).collect()
    })
    .await
}

pub async fn call_tool(
    options: &TransportOptions,
    tool_name: String,
    arguments: JsonObject,
) -> Result<CallOutput> {
    run_with_peer(options, "call tool", |peer| async move {
        let result = peer
            .call_tool(CallToolRequestParams::new(tool_name).with_arguments(arguments))
            .await
            .context("send tools/call")?;
        convert_call_output(result)
    })
    .await
}

pub async fn call_tool_batch(
    options: &TransportOptions,
    calls: Vec<BatchToolCall>,
    continue_on_error: bool,
) -> Result<Vec<BatchToolOutput>> {
    run_with_peer(options, "run tool batch", |peer| async move {
        let mut outputs = Vec::with_capacity(calls.len());
        for call in calls {
            let name = call.name;
            let result = peer
                .call_tool(CallToolRequestParams::new(name.clone()).with_arguments(call.arguments))
                .await
                .context("send tools/call")
                .and_then(convert_call_output);
            match result {
                Ok(result) => outputs.push(BatchToolOutput {
                    name,
                    ok: !result.is_error,
                    result: Some(result),
                    error: None,
                }),
                Err(error) if continue_on_error => outputs.push(BatchToolOutput {
                    name,
                    ok: false,
                    result: None,
                    error: Some(format!("{error:#}")),
                }),
                Err(error) => return Err(error),
            }
        }
        Ok(outputs)
    })
    .await
}

pub async fn list_resources(options: &TransportOptions) -> Result<Vec<ResourceInfo>> {
    run_with_peer(options, "list resources", |peer| async move {
        let resources = peer
            .list_all_resources()
            .await
            .context("send resources/list")?;
        resources.into_iter().map(convert_resource).collect()
    })
    .await
}

pub async fn list_resource_templates(
    options: &TransportOptions,
) -> Result<Vec<ResourceTemplateInfo>> {
    run_with_peer(options, "list resource templates", |peer| async move {
        let templates = peer
            .list_all_resource_templates()
            .await
            .context("send resources/templates/list")?;
        templates
            .into_iter()
            .map(convert_resource_template)
            .collect()
    })
    .await
}

pub async fn read_resource(options: &TransportOptions, uri: String) -> Result<ReadResourceOutput> {
    run_with_peer(options, "read resource", |peer| async move {
        let result = peer
            .read_resource(ReadResourceRequestParams::new(uri))
            .await
            .context("send resources/read")?;
        convert_read_resource_output(result)
    })
    .await
}

pub async fn list_prompts(options: &TransportOptions) -> Result<Vec<PromptInfo>> {
    run_with_peer(options, "list prompts", |peer| async move {
        let prompts = peer.list_all_prompts().await.context("send prompts/list")?;
        prompts.into_iter().map(convert_prompt).collect()
    })
    .await
}

pub async fn get_prompt(
    options: &TransportOptions,
    prompt_name: String,
    arguments: JsonObject,
) -> Result<PromptOutput> {
    run_with_peer(options, "get prompt", |peer| async move {
        let mut params = GetPromptRequestParams::new(prompt_name);
        if !arguments.is_empty() {
            params = params.with_arguments(arguments);
        }
        let result = peer.get_prompt(params).await.context("send prompts/get")?;
        convert_prompt_output(result)
    })
    .await
}

pub async fn complete_prompt(
    options: &TransportOptions,
    prompt_name: String,
    argument_name: String,
    value: String,
    context: JsonObject,
) -> Result<CompletionOutput> {
    run_with_peer(options, "complete prompt argument", |peer| async move {
        let result = peer
            .complete_prompt_argument(
                prompt_name,
                argument_name,
                value,
                completion_context(context),
            )
            .await
            .context("send completion/complete for prompt")?;
        convert_completion_output(result)
    })
    .await
}

pub async fn complete_resource(
    options: &TransportOptions,
    uri_template: String,
    argument_name: String,
    value: String,
    context: JsonObject,
) -> Result<CompletionOutput> {
    run_with_peer(options, "complete resource argument", |peer| async move {
        let result = peer
            .complete_resource_argument(
                uri_template,
                argument_name,
                value,
                completion_context(context),
            )
            .await
            .context("send completion/complete for resource")?;
        convert_completion_output(result)
    })
    .await
}

pub async fn inspect_server(options: &TransportOptions) -> Result<DoctorReport> {
    run_with_peer(options, "inspect server", |peer| async move {
        let server = peer.peer_info().map(raw_json).transpose()?;
        let capabilities = server
            .as_ref()
            .and_then(|value| value.get("capabilities"))
            .cloned();
        let tools = probe_count(peer.list_all_tools().await.map(|items| items.len()));
        let resources = probe_count(peer.list_all_resources().await.map(|items| items.len()));
        let resource_templates = probe_count(
            peer.list_all_resource_templates()
                .await
                .map(|items| items.len()),
        );
        let prompts = probe_count(peer.list_all_prompts().await.map(|items| items.len()));

        let mut warnings = Vec::new();
        if options.roots.is_empty() {
            warnings
                .push("no roots advertised; pass --root when a server needs roots/list".to_owned());
        }
        if let Some(capabilities) = &capabilities {
            add_capability_warnings(capabilities, &mut warnings);
        }

        Ok(DoctorReport {
            ok: true,
            endpoint: describe_endpoint(options),
            server,
            capabilities,
            tools,
            resources,
            resource_templates,
            prompts,
            warnings,
        })
    })
    .await
}

async fn run_with_peer<T, Op, Fut>(
    options: &TransportOptions,
    label: &'static str,
    operation: Op,
) -> Result<T>
where
    Op: FnOnce(Peer<RoleClient>) -> Fut,
    Fut: std::future::Future<Output = Result<T>>,
{
    tokio::time::timeout(Duration::from_secs(options.timeout_secs), async {
        match parse_transport(options)? {
            TransportConfig::Http {
                url,
                bearer,
                headers,
            } => {
                let transport = http_transport(url, bearer, headers);
                let mut client = McpcallClient::from_options(options)
                    .serve(transport)
                    .await
                    .context("initialize MCP server")?;
                let result = operation(client.peer().clone()).await;
                let _ = client.close_with_timeout(Duration::from_secs(2)).await;
                result
            }
            TransportConfig::Sse {
                url,
                bearer,
                headers,
            } => {
                let transport = LegacySseTransport::start(url, bearer, headers)
                    .await
                    .context("connect legacy SSE MCP server")?;
                let mut client = McpcallClient::from_options(options)
                    .serve(transport)
                    .await
                    .context("initialize MCP server")?;
                let result = operation(client.peer().clone()).await;
                let _ = client.close_with_timeout(Duration::from_secs(2)).await;
                result
            }
            TransportConfig::Stdio {
                program,
                args,
                cwd,
                env,
            } => {
                let transport =
                    stdio_transport(program, args, cwd, env).context("spawn stdio MCP server")?;
                let mut client = McpcallClient::from_options(options)
                    .serve(transport)
                    .await
                    .context("initialize MCP server")?;
                let result = operation(client.peer().clone()).await;
                let _ = client.close_with_timeout(Duration::from_secs(2)).await;
                result
            }
        }
    })
    .await
    .with_context(|| format!("{label} timed out after {}s", options.timeout_secs))?
}

fn parse_transport(options: &TransportOptions) -> Result<TransportConfig> {
    match &options.endpoint {
        Endpoint::Http {
            url,
            bearer,
            headers,
        } => Ok(TransportConfig::Http {
            url: url.clone(),
            bearer: bearer.clone(),
            headers: parse_headers(headers)?,
        }),
        Endpoint::Sse {
            url,
            bearer,
            headers,
        } => Ok(TransportConfig::Sse {
            url: url.clone(),
            bearer: bearer.clone(),
            headers: parse_headers(headers)?,
        }),
        Endpoint::Stdio { command, cwd, env } => {
            let mut parts = split_stdio_command(command)
                .with_context(|| format!("parse stdio command: {command}"))?;
            if parts.is_empty() {
                bail!("--stdio command cannot be empty");
            }
            let program = parts.remove(0);
            Ok(TransportConfig::Stdio {
                program,
                args: parts,
                cwd: cwd.clone(),
                env: env.clone(),
            })
        }
    }
}

fn split_stdio_command(command: &str) -> Result<Vec<String>> {
    #[cfg(windows)]
    {
        split_windows_command(command)
    }
    #[cfg(not(windows))]
    {
        shell_words::split(command).map_err(Into::into)
    }
}

#[cfg(windows)]
fn split_windows_command(command: &str) -> Result<Vec<String>> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut quote = None;

    for ch in command.chars() {
        match (quote, ch) {
            (Some(q), ch) if ch == q => quote = None,
            (Some(_), ch) => current.push(ch),
            (None, '"' | '\'') => quote = Some(ch),
            (None, ch) if ch.is_whitespace() => {
                if !current.is_empty() {
                    parts.push(std::mem::take(&mut current));
                }
            }
            (None, ch) => current.push(ch),
        }
    }

    if let Some(quote) = quote {
        bail!("unterminated {quote} quote in stdio command");
    }
    if !current.is_empty() {
        parts.push(current);
    }
    Ok(parts)
}

fn describe_endpoint(options: &TransportOptions) -> String {
    match &options.endpoint {
        Endpoint::Http { url, .. } => format!("http {url}"),
        Endpoint::Sse { url, .. } => format!("sse {url}"),
        Endpoint::Stdio { command, .. } => format!("stdio {command}"),
    }
}

fn probe_count<E>(result: std::result::Result<usize, E>) -> PrimitiveProbe
where
    E: std::fmt::Display,
{
    match result {
        Ok(count) => PrimitiveProbe {
            supported: true,
            count: Some(count),
            error: None,
        },
        Err(error) => PrimitiveProbe {
            supported: false,
            count: None,
            error: Some(error.to_string()),
        },
    }
}

fn add_capability_warnings(capabilities: &Value, warnings: &mut Vec<String>) {
    let supported = capabilities
        .as_object()
        .map(|items| items.keys().cloned().collect::<HashSet<_>>())
        .unwrap_or_default();
    if supported.contains("logging") {
        warnings.push(
            "server supports logging; mcpcall records logging notifications through tracing"
                .to_owned(),
        );
    }
    if supported.contains("completions") {
        warnings
            .push("server supports completions; use mcpcall complete prompt/resource".to_owned());
    }
    if supported.contains("tasks") {
        warnings.push("server exposes task capabilities; mcpcall can inspect them but does not manage task lifecycle yet".to_owned());
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

async fn open_legacy_sse_stream(
    client: &reqwest::Client,
    url: &str,
    headers: &HashMap<HeaderName, HeaderValue>,
    bearer: Option<String>,
) -> std::result::Result<LegacySseEventStream, LegacySseError> {
    let request = apply_reqwest_headers(
        client
            .get(url)
            .header(reqwest::header::ACCEPT, "text/event-stream"),
        headers,
        bearer,
    );
    let response = request.send().await?.error_for_status()?;
    match response.headers().get(reqwest::header::CONTENT_TYPE) {
        Some(content_type) if content_type.as_bytes().starts_with(b"text/event-stream") => {}
        Some(content_type) => {
            return Err(LegacySseError::new(format!(
                "unexpected SSE content type: {}",
                content_type.to_str().unwrap_or("<non-utf8>")
            )));
        }
        None => return Err(LegacySseError::new("missing SSE content type")),
    }
    Ok(SseStream::from_byte_stream(response.bytes_stream()).boxed())
}

fn apply_reqwest_headers(
    mut request: reqwest::RequestBuilder,
    headers: &HashMap<HeaderName, HeaderValue>,
    bearer: Option<String>,
) -> reqwest::RequestBuilder {
    for (name, value) in headers {
        request = request.header(name, value);
    }
    if let Some(token) = bearer {
        request = request.bearer_auth(token);
    }
    request
}

async fn wait_for_legacy_message_endpoint(
    base_url: &str,
    stream: &mut LegacySseEventStream,
) -> std::result::Result<String, LegacySseError> {
    while let Some(event) = stream.next().await {
        let event = event?;
        let Some("endpoint") = event.event.as_deref() else {
            continue;
        };
        let endpoint = event.data.unwrap_or_default();
        return resolve_legacy_message_endpoint(base_url, &endpoint);
    }
    Err(LegacySseError::new(
        "legacy SSE stream ended before endpoint event",
    ))
}

fn decode_legacy_sse_message(
    event: Sse,
) -> Option<std::result::Result<ServerJsonRpcMessage, LegacySseError>> {
    if matches!(event.event.as_deref(), Some("endpoint" | "ping")) {
        return None;
    }
    let data = event.data?;
    if data.trim().is_empty() {
        return None;
    }
    Some(serde_json::from_str(&data).map_err(LegacySseError::from))
}

fn resolve_legacy_message_endpoint(
    base_url: &str,
    endpoint: &str,
) -> std::result::Result<String, LegacySseError> {
    if endpoint.starts_with("http://") || endpoint.starts_with("https://") {
        endpoint.parse::<http::Uri>()?;
        return Ok(endpoint.to_owned());
    }

    let base = base_url.parse::<http::Uri>()?;
    let mut parts = base.into_parts();
    if endpoint.starts_with('?') {
        let base_path = parts
            .path_and_query
            .as_ref()
            .map(|value| value.path())
            .unwrap_or("/");
        parts.path_and_query = Some(format!("{base_path}{endpoint}").parse()?);
    } else {
        let path = if endpoint.starts_with('/') {
            endpoint.to_owned()
        } else {
            format!("/{endpoint}")
        };
        parts.path_and_query = Some(path.parse()?);
    }
    Ok(http::Uri::from_parts(parts)?.to_string())
}

fn stdio_transport(
    program: String,
    args: Vec<String>,
    cwd: Option<PathBuf>,
    env: Vec<KeyValue>,
) -> std::io::Result<TokioChildProcess> {
    let mut command = Command::new(resolve_program(&program));
    command.args(args);
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    for item in env {
        command.env(item.key, item.value);
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

fn parse_headers(values: &[KeyValue]) -> Result<HashMap<HeaderName, HeaderValue>> {
    let mut headers = HashMap::new();
    for item in values {
        let name = HeaderName::from_bytes(item.key.as_bytes())
            .with_context(|| format!("invalid HTTP header name: {}", item.key))?;
        let value = HeaderValue::from_str(&item.value)
            .with_context(|| format!("invalid value for HTTP header {}", item.key))?;
        headers.insert(name, value);
    }
    Ok(headers)
}

fn convert_tool(tool: Tool) -> Result<ToolInfo> {
    let input_schema = tool.schema_as_json_value();
    Ok(ToolInfo {
        name: tool.name.to_string(),
        description: tool.description.as_ref().map(ToString::to_string),
        input_schema,
        raw: raw_json(&tool)?,
    })
}

fn convert_resource(resource: Resource) -> Result<ResourceInfo> {
    Ok(ResourceInfo {
        uri: resource.uri.clone(),
        name: resource.name.clone(),
        title: resource.title.clone(),
        description: resource.description.clone(),
        mime_type: resource.mime_type.clone(),
        raw: raw_json(&resource)?,
    })
}

fn convert_resource_template(template: ResourceTemplate) -> Result<ResourceTemplateInfo> {
    Ok(ResourceTemplateInfo {
        uri_template: template.uri_template.clone(),
        name: template.name.clone(),
        title: template.title.clone(),
        description: template.description.clone(),
        mime_type: template.mime_type.clone(),
        raw: raw_json(&template)?,
    })
}

fn convert_prompt(prompt: Prompt) -> Result<PromptInfo> {
    let arguments = prompt
        .arguments
        .clone()
        .unwrap_or_default()
        .into_iter()
        .map(|argument| PromptArgumentInfo {
            name: argument.name,
            title: argument.title,
            description: argument.description,
            required: argument.required.unwrap_or(false),
        })
        .collect();

    Ok(PromptInfo {
        name: prompt.name.clone(),
        title: prompt.title.clone(),
        description: prompt.description.clone(),
        arguments,
        raw: raw_json(&prompt)?,
    })
}

fn convert_call_output(result: CallToolResult) -> Result<CallOutput> {
    Ok(CallOutput {
        is_error: result.is_error.unwrap_or(false),
        structured_content: result.structured_content.clone(),
        content: result
            .content
            .iter()
            .map(convert_content_block)
            .collect::<Result<Vec<_>>>()?,
        raw: raw_json(&result)?,
    })
}

fn convert_read_resource_output(result: ReadResourceResult) -> Result<ReadResourceOutput> {
    Ok(ReadResourceOutput {
        contents: result
            .contents
            .iter()
            .map(convert_resource_content)
            .collect::<Vec<_>>(),
        raw: raw_json(&result)?,
    })
}

fn convert_prompt_output(result: GetPromptResult) -> Result<PromptOutput> {
    Ok(PromptOutput {
        description: result.description.clone(),
        messages: result
            .messages
            .iter()
            .map(raw_json)
            .collect::<Result<Vec<_>>>()?,
        raw: raw_json(&result)?,
    })
}

fn convert_completion_output(result: CompletionInfo) -> Result<CompletionOutput> {
    Ok(CompletionOutput {
        values: result.values.clone(),
        total: result.total,
        has_more: result.has_more,
        raw: raw_json(&result)?,
    })
}

fn completion_context(arguments: JsonObject) -> Option<CompletionContext> {
    if arguments.is_empty() {
        return None;
    }
    Some(CompletionContext::with_arguments(
        arguments
            .into_iter()
            .map(|(key, value)| {
                let value = value
                    .as_str()
                    .map(str::to_owned)
                    .unwrap_or_else(|| value.to_string());
                (key, value)
            })
            .collect(),
    ))
}

fn convert_content_block(content: &Content) -> Result<ContentBlock> {
    match &content.raw {
        RawContent::Text(text) => Ok(ContentBlock::Text {
            text: text.text.clone(),
        }),
        RawContent::Image(image) => Ok(ContentBlock::Image {
            mime_type: image.mime_type.clone(),
            data: image.data.clone(),
        }),
        RawContent::Audio(audio) => Ok(ContentBlock::Audio {
            mime_type: audio.mime_type.clone(),
            data: audio.data.clone(),
        }),
        RawContent::Resource(resource) => Ok(match &resource.resource {
            ResourceContents::TextResourceContents {
                uri,
                mime_type,
                text,
                ..
            } => ContentBlock::ResourceText {
                uri: uri.clone(),
                mime_type: mime_type.clone(),
                text: text.clone(),
            },
            ResourceContents::BlobResourceContents {
                uri,
                mime_type,
                blob,
                ..
            } => ContentBlock::ResourceBlob {
                uri: uri.clone(),
                mime_type: mime_type.clone(),
                blob: blob.clone(),
            },
        }),
        RawContent::ResourceLink(link) => Ok(ContentBlock::ResourceLink {
            uri: link.uri.clone(),
            name: link.name.clone(),
            description: link.description.clone(),
            mime_type: link.mime_type.clone(),
        }),
    }
}

fn convert_resource_content(content: &ResourceContents) -> ResourceContent {
    match content {
        ResourceContents::TextResourceContents {
            uri,
            mime_type,
            text,
            ..
        } => ResourceContent::Text {
            uri: uri.clone(),
            mime_type: mime_type.clone(),
            text: text.clone(),
        },
        ResourceContents::BlobResourceContents {
            uri,
            mime_type,
            blob,
            ..
        } => ResourceContent::Blob {
            uri: uri.clone(),
            mime_type: mime_type.clone(),
            blob: blob.clone(),
        },
    }
}

fn raw_json(value: &impl Serialize) -> Result<Value> {
    serde_json::to_value(value).context("serialize MCP value")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    #[test]
    fn windows_stdio_split_preserves_backslashes() {
        let parts =
            split_windows_command(r#"python C:\Temp\server.py --flag "two words""#).unwrap();

        assert_eq!(parts[0], "python");
        assert_eq!(parts[1], r#"C:\Temp\server.py"#);
        assert_eq!(parts[3], "two words");
    }

    #[test]
    fn legacy_sse_message_endpoint_resolution_matches_mcp_endpoint_events() {
        assert_eq!(
            resolve_legacy_message_endpoint("https://localhost/sse", "?sessionId=x").unwrap(),
            "https://localhost/sse?sessionId=x"
        );
        assert_eq!(
            resolve_legacy_message_endpoint("https://localhost/sse", "message?sessionId=x")
                .unwrap(),
            "https://localhost/message?sessionId=x"
        );
        assert_eq!(
            resolve_legacy_message_endpoint("https://localhost/sse", "/xxx?sessionId=x").unwrap(),
            "https://localhost/xxx?sessionId=x"
        );
    }
}
