use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolInfo {
    pub name: String,
    pub description: Option<String>,
    pub input_schema: Value,
    pub raw: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceInfo {
    pub uri: String,
    pub name: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub mime_type: Option<String>,
    pub raw: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceTemplateInfo {
    pub uri_template: String,
    pub name: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub mime_type: Option<String>,
    pub raw: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptInfo {
    pub name: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub arguments: Vec<PromptArgumentInfo>,
    pub raw: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptArgumentInfo {
    pub name: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub required: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CallOutput {
    pub is_error: bool,
    pub structured_content: Option<Value>,
    pub content: Vec<ContentBlock>,
    pub raw: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    Text {
        text: String,
    },
    Image {
        mime_type: String,
        data: String,
    },
    Audio {
        mime_type: String,
        data: String,
    },
    ResourceText {
        uri: String,
        mime_type: Option<String>,
        text: String,
    },
    ResourceBlob {
        uri: String,
        mime_type: Option<String>,
        blob: String,
    },
    ResourceLink {
        uri: String,
        name: String,
        description: Option<String>,
        mime_type: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadResourceOutput {
    pub contents: Vec<ResourceContent>,
    pub raw: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ResourceContent {
    Text {
        uri: String,
        mime_type: Option<String>,
        text: String,
    },
    Blob {
        uri: String,
        mime_type: Option<String>,
        blob: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptOutput {
    pub description: Option<String>,
    pub messages: Vec<Value>,
    pub raw: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompletionOutput {
    pub values: Vec<String>,
    pub total: Option<u32>,
    pub has_more: Option<bool>,
    pub raw: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DoctorReport {
    pub ok: bool,
    pub endpoint: String,
    pub server: Option<Value>,
    pub capabilities: Option<Value>,
    pub tools: PrimitiveProbe,
    pub resources: PrimitiveProbe,
    pub resource_templates: PrimitiveProbe,
    pub prompts: PrimitiveProbe,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrimitiveProbe {
    pub supported: bool,
    pub count: Option<usize>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchToolCall {
    #[serde(alias = "tool")]
    pub name: String,
    #[serde(default)]
    pub arguments: serde_json::Map<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchToolOutput {
    pub name: String,
    pub ok: bool,
    pub result: Option<CallOutput>,
    pub error: Option<String>,
}
