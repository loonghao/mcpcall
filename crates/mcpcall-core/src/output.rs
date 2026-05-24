use anyhow::Result;
use serde_json::{Value, json};

use crate::model::{
    BatchToolOutput, CallOutput, CompletionOutput, ContentBlock, DoctorReport, PromptInfo,
    PromptOutput, ReadResourceOutput, ResourceContent, ResourceInfo, ResourceTemplateInfo,
    ToolInfo,
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
        if let Some(title) = tool.raw.get("title").and_then(Value::as_str) {
            println!("  title: {title}");
        }
        if let Some(description) = &tool.description {
            println!("  {}", one_line(description));
        }
        print_schema_summary(&tool.input_schema);
        if let Some(output_schema) = tool.raw.get("outputSchema") {
            print_output_schema_summary(output_schema);
        }
        print_annotations(tool.raw.get("annotations"));
        if schema {
            println!(
                "  input schema: {}",
                serde_json::to_string_pretty(&tool.input_schema)?
                    .lines()
                    .collect::<Vec<_>>()
                    .join("\n          ")
            );
            if let Some(output_schema) = tool.raw.get("outputSchema") {
                println!(
                    "  output schema: {}",
                    serde_json::to_string_pretty(output_schema)?
                        .lines()
                        .collect::<Vec<_>>()
                        .join("\n           ")
                );
            }
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

pub fn print_completion_result(result: &CompletionOutput, as_json: bool) -> Result<()> {
    if as_json {
        print_json(&result.raw)?;
        return Ok(());
    }

    for value in &result.values {
        println!("{value}");
    }
    if result.values.is_empty() {
        println!("No completions.");
    }
    Ok(())
}

pub fn print_doctor_report(report: &DoctorReport, as_json: bool) -> Result<()> {
    if as_json {
        print_json_value(report)?;
        return Ok(());
    }

    println!("endpoint: {}", report.endpoint);
    println!("initialize: {}", if report.ok { "ok" } else { "failed" });
    if let Some(server) = &report.server {
        if let Some(name) = server
            .get("serverInfo")
            .and_then(|info| info.get("name"))
            .and_then(Value::as_str)
        {
            println!("server: {name}");
        }
        if let Some(version) = server
            .get("serverInfo")
            .and_then(|info| info.get("version"))
            .and_then(Value::as_str)
        {
            println!("version: {version}");
        }
    }
    print_probe("tools", &report.tools);
    print_probe("resources", &report.resources);
    print_probe("resource templates", &report.resource_templates);
    print_probe("prompts", &report.prompts);
    for warning in &report.warnings {
        println!("warning: {warning}");
    }
    Ok(())
}

pub fn print_batch_results(results: &[BatchToolOutput], as_json: bool) -> Result<()> {
    if as_json {
        print_json_value(results)?;
        return Ok(());
    }

    for (index, item) in results.iter().enumerate() {
        if index > 0 {
            println!();
        }
        println!("== {} ==", item.name);
        if let Some(error) = &item.error {
            println!("error: {error}");
        } else if let Some(result) = &item.result {
            print_call_result(result, false)?;
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

fn print_output_schema_summary(schema: &Value) {
    let Some(properties) = schema.get("properties").and_then(Value::as_object) else {
        if schema.get("type").is_some() {
            println!("  output: {}", schema_type(schema));
        }
        return;
    };
    if properties.is_empty() {
        println!("  output: none");
        return;
    }
    let parts = properties
        .iter()
        .map(|(name, property)| format!("{name}:{}", schema_type(property)))
        .collect::<Vec<_>>();
    println!("  output: {}", parts.join(", "));
}

fn print_annotations(annotations: Option<&Value>) {
    let Some(annotations) = annotations.and_then(Value::as_object) else {
        return;
    };
    if annotations.is_empty() {
        return;
    }
    let parts = annotations
        .iter()
        .filter_map(|(key, value)| {
            if key == "title" {
                None
            } else if let Some(value) = value.as_bool() {
                Some(format!("{key}={value}"))
            } else if let Some(value) = value.as_str() {
                Some(format!("{key}={value}"))
            } else {
                Some(format!("{key}={}", one_line(&value.to_string())))
            }
        })
        .collect::<Vec<_>>();
    if !parts.is_empty() {
        println!("  annotations: {}", parts.join(", "));
    }
}

pub fn print_json_value(value: &(impl serde::Serialize + ?Sized)) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}

fn print_probe(label: &str, probe: &crate::model::PrimitiveProbe) {
    match (probe.supported, probe.count, &probe.error) {
        (true, Some(count), _) => println!("{label}: ok ({count})"),
        (true, None, _) => println!("{label}: ok"),
        (false, _, Some(error)) => println!("{label}: unavailable ({})", one_line(error)),
        (false, _, None) => println!("{label}: unavailable"),
    }
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
