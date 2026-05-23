use anyhow::Result;
use serde_json::{Value, json};

use crate::model::{
    CallOutput, ContentBlock, PromptInfo, PromptOutput, ReadResourceOutput, ResourceContent,
    ResourceInfo, ResourceTemplateInfo, ToolInfo,
};

pub fn print_tools(tools: &[ToolInfo], as_json: bool, schema: bool, brief: bool) -> Result<()> {
    if as_json {
        print_json(&tools_json_document(tools))?;
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
        print_schema_summary(&tool.input_schema);
        if schema {
            println!(
                "  schema: {}",
                serde_json::to_string_pretty(&tool.input_schema)?
                    .lines()
                    .collect::<Vec<_>>()
                    .join("\n          ")
            );
        }
    }

    Ok(())
}

fn tools_json_document(tools: &[ToolInfo]) -> Value {
    json!({
        "count": tools.len(),
        "tools": tools.iter().map(|tool| &tool.raw).collect::<Vec<_>>(),
    })
}

pub fn print_resources(resources: &[ResourceInfo], as_json: bool, brief: bool) -> Result<()> {
    if as_json {
        print_json(&json!({ "count": resources.len(), "resources": resources }))?;
        return Ok(());
    }

    if resources.is_empty() {
        println!("No resources exposed.");
        return Ok(());
    }

    for resource in resources {
        println!("{}", resource.uri);
        if brief {
            continue;
        }
        println!("  name: {}", resource.name);
        if let Some(title) = &resource.title {
            println!("  title: {title}");
        }
        if let Some(mime_type) = &resource.mime_type {
            println!("  mime: {mime_type}");
        }
        if let Some(description) = &resource.description {
            println!("  {}", one_line(description));
        }
    }

    Ok(())
}

pub fn print_resource_templates(
    templates: &[ResourceTemplateInfo],
    as_json: bool,
    brief: bool,
) -> Result<()> {
    if as_json {
        print_json(&json!({ "count": templates.len(), "resourceTemplates": templates }))?;
        return Ok(());
    }

    if templates.is_empty() {
        println!("No resource templates exposed.");
        return Ok(());
    }

    for template in templates {
        println!("{}", template.uri_template);
        if brief {
            continue;
        }
        println!("  name: {}", template.name);
        if let Some(title) = &template.title {
            println!("  title: {title}");
        }
        if let Some(mime_type) = &template.mime_type {
            println!("  mime: {mime_type}");
        }
        if let Some(description) = &template.description {
            println!("  {}", one_line(description));
        }
    }

    Ok(())
}

pub fn print_prompts(prompts: &[PromptInfo], as_json: bool, brief: bool) -> Result<()> {
    if as_json {
        print_json(&json!({ "count": prompts.len(), "prompts": prompts }))?;
        return Ok(());
    }

    if prompts.is_empty() {
        println!("No prompts exposed.");
        return Ok(());
    }

    for prompt in prompts {
        println!("{}", prompt.name);
        if brief {
            continue;
        }
        if let Some(description) = &prompt.description {
            println!("  {}", one_line(description));
        }
        if prompt.arguments.is_empty() {
            println!("  args: none");
        } else {
            let args = prompt
                .arguments
                .iter()
                .map(|arg| {
                    if arg.required {
                        arg.name.clone()
                    } else {
                        format!("{}?", arg.name)
                    }
                })
                .collect::<Vec<_>>();
            println!("  args: {}", args.join(", "));
        }
    }

    Ok(())
}

pub fn print_call_result(result: &CallOutput, as_json: bool) -> Result<()> {
    if as_json {
        print_json(&result.raw)?;
        return Ok(());
    }

    if let Some(value) = &result.structured_content {
        print_json(value)?;
        return Ok(());
    }

    print_content_blocks(&result.content)
}

pub fn print_read_resource(result: &ReadResourceOutput, as_json: bool) -> Result<()> {
    if as_json {
        print_json(&result.raw)?;
        return Ok(());
    }

    for (index, item) in result.contents.iter().enumerate() {
        if index > 0 {
            println!();
        }
        match item {
            ResourceContent::Text { text, .. } => println!("{text}"),
            ResourceContent::Blob {
                uri,
                mime_type,
                blob,
            } => print_json(&json!({
                "type": "blob",
                "uri": uri,
                "mimeType": mime_type,
                "blob": blob,
            }))?,
        }
    }

    Ok(())
}

pub fn print_prompt_result(result: &PromptOutput, as_json: bool) -> Result<()> {
    if as_json {
        print_json(&result.raw)?;
        return Ok(());
    }

    if let Some(description) = &result.description {
        println!("{}", one_line(description));
    }
    for (index, message) in result.messages.iter().enumerate() {
        if index > 0 || result.description.is_some() {
            println!();
        }
        if let Some(text) = message
            .get("content")
            .and_then(|content| content.get("text"))
            .and_then(Value::as_str)
        {
            let role = message
                .get("role")
                .and_then(Value::as_str)
                .unwrap_or("message");
            println!("{role}: {text}");
        } else {
            print_json(message)?;
        }
    }

    Ok(())
}

fn print_content_blocks(content: &[ContentBlock]) -> Result<()> {
    for (index, item) in content.iter().enumerate() {
        if index > 0 {
            println!();
        }
        match item {
            ContentBlock::Text { text } => println!("{text}"),
            ContentBlock::Image { mime_type, data } => {
                print_json(&json!({ "type": "image", "mimeType": mime_type, "data": data }))?;
            }
            ContentBlock::Audio { mime_type, data } => {
                print_json(&json!({ "type": "audio", "mimeType": mime_type, "data": data }))?;
            }
            ContentBlock::ResourceText {
                uri,
                mime_type,
                text,
            } => print_json(&json!({
                "type": "resource",
                "uri": uri,
                "mimeType": mime_type,
                "text": text,
            }))?,
            ContentBlock::ResourceBlob {
                uri,
                mime_type,
                blob,
            } => print_json(&json!({
                "type": "resource",
                "uri": uri,
                "mimeType": mime_type,
                "blob": blob,
            }))?,
            ContentBlock::ResourceLink {
                uri,
                name,
                description,
                mime_type,
            } => print_json(&json!({
                "type": "resource_link",
                "uri": uri,
                "name": name,
                "description": description,
                "mimeType": mime_type,
            }))?,
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

fn print_json(value: &Value) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tools_json_preserves_raw_tool_fields_at_top_level() {
        let raw = json!({
            "name": "dcc_status",
            "description": "Status",
            "inputSchema": {"type": "object"},
            "title": "DCC Status",
            "outputSchema": {"type": "object", "properties": {"ok": {"type": "boolean"}}},
            "annotations": {"readOnlyHint": true},
            "execution": {"affinity": "main"},
            "icons": [{"src": "https://example.invalid/icon.png"}],
            "_meta": {"vendor": "dcc-mcp"}
        });
        let tools = vec![ToolInfo {
            name: "dcc_status".to_owned(),
            description: Some("Status".to_owned()),
            input_schema: json!({"type": "object"}),
            raw: raw.clone(),
        }];

        let output = tools_json_document(&tools);

        assert_eq!(output["tools"][0], raw);
        assert!(output["tools"][0].get("raw").is_none());
        assert_eq!(output["tools"][0]["title"], "DCC Status");
        assert_eq!(output["tools"][0]["_meta"]["vendor"], "dcc-mcp");
    }
}
