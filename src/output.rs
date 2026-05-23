use anyhow::{Result, bail};
use rmcp::model::{CallToolResult, Content, RawContent, ResourceContents, Tool};
use serde_json::{Value, json};

pub fn print_tools(tools: &[Tool], as_json: bool, schema: bool, brief: bool) -> Result<()> {
    if as_json {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "count": tools.len(),
                "tools": tools,
            }))?
        );
        return Ok(());
    }

    if tools.is_empty() {
        println!("No tools exposed.");
        return Ok(());
    }

    for tool in tools {
        println!("{}", tool.name);
        if brief {
            continue;
        }
        if let Some(description) = &tool.description {
            println!("  {}", one_line(description));
        }
        print_schema_summary(&tool.schema_as_json_value());
        if schema {
            println!(
                "  schema: {}",
                serde_json::to_string_pretty(&tool.schema_as_json_value())?
                    .lines()
                    .collect::<Vec<_>>()
                    .join("\n          ")
            );
        }
    }

    Ok(())
}

pub fn print_call_result(result: &CallToolResult, as_json: bool) -> Result<()> {
    if as_json {
        println!("{}", serde_json::to_string_pretty(result)?);
        return Ok(());
    }

    if let Some(value) = &result.structured_content {
        println!("{}", serde_json::to_string_pretty(value)?);
        return Ok(());
    }

    if result.content.is_empty() {
        return Ok(());
    }

    for (index, item) in result.content.iter().enumerate() {
        if index > 0 {
            println!();
        }
        print_content(item)?;
    }

    Ok(())
}

fn print_content(content: &Content) -> Result<()> {
    match &content.raw {
        RawContent::Text(text) => {
            println!("{}", text.text);
        }
        RawContent::Image(image) => {
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "type": "image",
                    "mimeType": image.mime_type,
                    "data": image.data,
                }))?
            );
        }
        RawContent::Audio(audio) => {
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "type": "audio",
                    "mimeType": audio.mime_type,
                    "data": audio.data,
                }))?
            );
        }
        RawContent::Resource(resource) => match &resource.resource {
            ResourceContents::TextResourceContents {
                uri,
                mime_type,
                text,
                ..
            } => {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&json!({
                        "type": "resource",
                        "uri": uri,
                        "mimeType": mime_type,
                        "text": text,
                    }))?
                );
            }
            ResourceContents::BlobResourceContents {
                uri,
                mime_type,
                blob,
                ..
            } => {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&json!({
                        "type": "resource",
                        "uri": uri,
                        "mimeType": mime_type,
                        "blob": blob,
                    }))?
                );
            }
        },
        RawContent::ResourceLink(link) => {
            println!("{}", serde_json::to_string_pretty(link)?);
        }
    }
    Ok(())
}

fn print_schema_summary(schema: &Value) {
    let Some(properties) = schema.get("properties").and_then(Value::as_object) else {
        return;
    };
    if properties.is_empty() {
        println!("  params: none");
        return;
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

    let mut parts = Vec::new();
    for (name, property) in properties {
        let marker = if required.contains(name.as_str()) {
            ""
        } else {
            "?"
        };
        parts.push(format!("{name}{marker}:{}", schema_type(property)));
    }
    println!("  params: {}", parts.join(", "));
}

fn schema_type(value: &Value) -> &str {
    if let Some(type_name) = value.get("type").and_then(Value::as_str) {
        return type_name;
    }
    if value.get("enum").is_some() {
        return "enum";
    }
    if value.get("anyOf").is_some() {
        return "any";
    }
    if value.get("oneOf").is_some() {
        return "one";
    }
    "unknown"
}

fn one_line(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn ensure_json_object(value: Value, source: &str) -> Result<serde_json::Map<String, Value>> {
    match value {
        Value::Object(map) => Ok(map),
        _ => bail!("{source} must be a JSON object"),
    }
}
