use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::{Context, Result, bail};
use http::{HeaderName, HeaderValue};
use mcpcall_core::{
    CallOutput, ContentBlock, Endpoint, KeyValue, PromptArgumentInfo, PromptInfo, PromptOutput,
    ReadResourceOutput, ResourceContent, ResourceInfo, ResourceTemplateInfo, ToolInfo,
    TransportOptions,
};
use rmcp::{
    ServiceExt,
    model::{
        CallToolRequestParams, CallToolResult, Content, GetPromptRequestParams, GetPromptResult,
        JsonObject, Prompt, RawContent, ReadResourceRequestParams, ReadResourceResult, Resource,
        ResourceContents, ResourceTemplate, Tool,
    },
    service::{Peer, RoleClient},
    transport::{
        StreamableHttpClientTransport, TokioChildProcess,
        streamable_http_client::StreamableHttpClientTransportConfig,
    },
};
use serde::Serialize;
use serde_json::Value;
use tokio::process::Command;

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
        env: Vec<KeyValue>,
    },
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
                let mut client = ().serve(transport).await.context("initialize MCP server")?;
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
                let mut client = ().serve(transport).await.context("initialize MCP server")?;
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
        Endpoint::Stdio { command, cwd, env } => {
            let mut parts = shell_words::split(command)
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
