use std::{
    collections::BTreeMap,
    env,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::transport::{Endpoint, KeyValue, TransportOptions, key_values_from_map};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct McpcallConfig {
    #[serde(default, alias = "servers")]
    pub mcp_servers: BTreeMap<String, ConfigServer>,
}

impl McpcallConfig {
    pub fn from_json_str(input: &str) -> Result<Self> {
        serde_json::from_str(input).context("parse MCP config JSON")
    }

    pub fn to_pretty_json(&self) -> Result<String> {
        serde_json::to_string_pretty(self).context("serialize MCP config JSON")
    }

    pub fn server_names(&self) -> Vec<&str> {
        self.mcp_servers.keys().map(String::as_str).collect()
    }

    pub fn server(&self, name: &str) -> Result<&ConfigServer> {
        self.mcp_servers
            .get(name)
            .with_context(|| format!("server {name:?} not found in config"))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ConfigServer {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, String>,
    #[serde(
        default,
        alias = "baseUrl",
        alias = "serverUrl",
        alias = "httpUrl",
        alias = "mcpUrl",
        skip_serializing_if = "Option::is_none"
    )]
    pub url: Option<String>,
    #[serde(default, alias = "sseUrl", skip_serializing_if = "Option::is_none")]
    pub sse_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transport: Option<String>,
    #[serde(
        default,
        rename = "type",
        alias = "transportType",
        skip_serializing_if = "Option::is_none"
    )]
    pub transport_type: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub headers: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bearer: Option<String>,
    #[serde(
        default,
        alias = "bearerTokenEnv",
        skip_serializing_if = "Option::is_none"
    )]
    pub bearer_env: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub roots: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub oauth: Option<OAuthConfig>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct OAuthConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_secret_env: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scopes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ConfigOverlay {
    pub headers: Vec<KeyValue>,
    pub env: Vec<KeyValue>,
    pub bearer: Option<String>,
    pub bearer_env: Option<String>,
    pub roots: Vec<String>,
    pub timeout_secs: Option<u64>,
}

impl ConfigServer {
    pub fn to_transport_options(&self, overlay: ConfigOverlay) -> Result<TransportOptions> {
        let timeout_secs = overlay.timeout_secs.unwrap_or(30);
        let roots = merge_roots(&normalize_roots(&self.roots)?, &overlay.roots);
        let bearer = resolve_bearer(
            overlay.bearer.as_ref().or(self.bearer.as_ref()),
            overlay.bearer_env.as_ref().or(self.bearer_env.as_ref()),
        )?;

        let transport_hint = self
            .transport
            .as_deref()
            .or(self.transport_type.as_deref())
            .map(str::to_ascii_lowercase);

        let endpoint = if let Some(url) = &self.sse_url {
            Endpoint::Sse {
                url: expand_env_value(url)?,
                bearer,
                headers: merge_key_values(&self.headers, &overlay.headers)?,
            }
        } else if matches!(transport_hint.as_deref(), Some("sse"))
            || self.url.as_deref().is_some_and(|url| url.ends_with("/sse"))
        {
            Endpoint::Sse {
                url: self
                    .url
                    .as_deref()
                    .map(expand_env_value)
                    .transpose()?
                    .context("SSE config entry requires url or sseUrl")?,
                bearer,
                headers: merge_key_values(&self.headers, &overlay.headers)?,
            }
        } else if let Some(url) = &self.url {
            Endpoint::Http {
                url: expand_env_value(url)?,
                bearer,
                headers: merge_key_values(&self.headers, &overlay.headers)?,
            }
        } else if let Some(command) = &self.command {
            Endpoint::Stdio {
                command: stdio_command(
                    &expand_env_value(command)?,
                    &expand_env_values(&self.args)?,
                ),
                cwd: self.cwd.as_deref().map(expand_path).transpose()?,
                env: merge_key_values(&self.env, &overlay.env)?,
            }
        } else {
            bail!("config server entry must define url, sseUrl, or command");
        };

        Ok(TransportOptions {
            endpoint,
            timeout_secs,
            roots,
        })
    }
}

pub fn resolve_bearer(
    bearer: Option<&String>,
    bearer_env: Option<&String>,
) -> Result<Option<String>> {
    if let Some(token) = bearer {
        return Ok(Some(expand_env_value(token)?));
    }
    if let Some(var) = bearer_env {
        let token = std::env::var(var)
            .with_context(|| format!("read bearer token from environment variable {var}"))?;
        return Ok(Some(token));
    }
    Ok(None)
}

pub fn merge_key_values(
    base: &BTreeMap<String, String>,
    overlay: &[KeyValue],
) -> Result<Vec<KeyValue>> {
    let mut values = key_values_from_map(base)
        .into_iter()
        .map(|item| {
            Ok(KeyValue {
                key: item.key,
                value: expand_env_value(&item.value)?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    for item in overlay {
        if let Some(existing) = values.iter_mut().find(|value| value.key == item.key) {
            existing.value = item.value.clone();
        } else {
            values.push(item.clone());
        }
    }
    Ok(values)
}

pub fn stdio_command(command: &str, args: &[String]) -> String {
    if args.is_empty() {
        command.to_owned()
    } else {
        std::iter::once(command.to_owned())
            .chain(args.iter().map(|arg| shell_quote(arg)))
            .collect::<Vec<_>>()
            .join(" ")
    }
}

fn shell_quote(value: &str) -> String {
    if value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || "-_./:\\=".contains(ch))
    {
        value.to_owned()
    } else {
        format!("'{}'", value.replace('\'', r#"'\''"#))
    }
}

fn merge_roots(base: &[String], overlay: &[String]) -> Vec<String> {
    let mut roots = base.to_vec();
    for root in overlay {
        if !roots.contains(root) {
            roots.push(root.clone());
        }
    }
    roots
}

fn normalize_roots(values: &[String]) -> Result<Vec<String>> {
    values
        .iter()
        .map(|value| normalize_root(&expand_env_value(value)?))
        .collect()
}

fn normalize_root(value: &str) -> Result<String> {
    if value.contains("://") {
        return Ok(value.to_owned());
    }
    path_to_file_uri(&PathBuf::from(value))
}

fn expand_path(path: &Path) -> Result<PathBuf> {
    Ok(PathBuf::from(expand_env_value(&path.to_string_lossy())?))
}

fn path_to_file_uri(path: &Path) -> Result<String> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        env::current_dir()
            .context("resolve current directory for config root")?
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

fn expand_env_values(values: &[String]) -> Result<Vec<String>> {
    values.iter().map(|value| expand_env_value(value)).collect()
}

fn expand_env_value(value: &str) -> Result<String> {
    expand_env_colon_placeholders(&expand_braced_env_placeholders(value)?)
}

fn expand_braced_env_placeholders(value: &str) -> Result<String> {
    let mut output = String::new();
    let mut rest = value;
    while let Some(start) = rest.find("${") {
        output.push_str(&rest[..start]);
        let placeholder_start = start + 2;
        let Some(end) = rest[placeholder_start..].find('}') else {
            bail!("unterminated environment placeholder in config value");
        };
        let expression = &rest[placeholder_start..placeholder_start + end];
        output.push_str(&resolve_env_expression(expression)?);
        rest = &rest[placeholder_start + end + 1..];
    }
    output.push_str(rest);
    Ok(output)
}

fn expand_env_colon_placeholders(value: &str) -> Result<String> {
    let mut output = String::new();
    let mut rest = value;
    while let Some(start) = rest.find("$env:") {
        output.push_str(&rest[..start]);
        let name_start = start + "$env:".len();
        let name_len = rest[name_start..]
            .chars()
            .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_')
            .map(char::len_utf8)
            .sum::<usize>();
        if name_len == 0 {
            output.push_str("$env:");
            rest = &rest[name_start..];
            continue;
        }
        let name = &rest[name_start..name_start + name_len];
        output.push_str(
            &env::var(name).with_context(|| format!("environment variable {name} is not set"))?,
        );
        rest = &rest[name_start + name_len..];
    }
    output.push_str(rest);
    Ok(output)
}

fn resolve_env_expression(expression: &str) -> Result<String> {
    let (name, fallback) = expression
        .split_once(":-")
        .map_or((expression, None), |(name, fallback)| {
            (name, Some(fallback))
        });
    if name.is_empty() {
        bail!("empty environment variable placeholder in config value");
    }
    match env::var(name) {
        Ok(value) if !value.is_empty() => Ok(value),
        _ if fallback.is_some() => Ok(fallback.unwrap().to_owned()),
        Ok(_) => bail!("environment variable {name} is empty"),
        Err(error) => Err(error).with_context(|| format!("environment variable {name} is not set")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_common_mcp_servers_config() {
        let config = McpcallConfig::from_json_str(
            r#"{
              "mcpServers": {
                "dcc": {
                  "command": "python",
                  "args": ["-m", "server"],
                  "env": {"TOKEN": "abc"}
                }
              }
            }"#,
        )
        .unwrap();

        let server = config.server("dcc").unwrap();
        let options = server
            .to_transport_options(ConfigOverlay::default())
            .unwrap();

        assert_eq!(options.timeout_secs, 30);
        match options.endpoint {
            Endpoint::Stdio { command, env, .. } => {
                assert_eq!(command, "python -m server");
                assert_eq!(env[0].key, "TOKEN");
            }
            _ => panic!("expected stdio endpoint"),
        }
    }

    #[test]
    fn resolves_http_config_with_overlay() {
        let mut headers = BTreeMap::new();
        headers.insert("X-Base".to_owned(), "old".to_owned());
        let server = ConfigServer {
            url: Some("http://127.0.0.1:8765/mcp".to_owned()),
            headers,
            roots: vec!["file:///repo".to_owned()],
            ..ConfigServer::default()
        };
        let options = server
            .to_transport_options(ConfigOverlay {
                headers: vec![
                    KeyValue {
                        key: "X-Base".to_owned(),
                        value: "new".to_owned(),
                    },
                    KeyValue {
                        key: "X-Trace".to_owned(),
                        value: "1".to_owned(),
                    },
                ],
                roots: vec!["file:///repo".to_owned(), "file:///other".to_owned()],
                timeout_secs: Some(5),
                ..ConfigOverlay::default()
            })
            .unwrap();

        assert_eq!(options.timeout_secs, 5);
        assert_eq!(options.roots, vec!["file:///repo", "file:///other"]);
        match options.endpoint {
            Endpoint::Http { headers, .. } => {
                assert_eq!(headers.len(), 2);
                assert_eq!(headers[0].value, "new");
            }
            _ => panic!("expected http endpoint"),
        }
    }

    #[test]
    fn accepts_remote_aliases_and_env_fallbacks() {
        let config = McpcallConfig::from_json_str(
            r#"{
              "mcpServers": {
                "linear": {
                  "baseUrl": "https://${__MCPCALL_TEST_HOST_NOT_SET__:-example.com}/mcp",
                  "transport": "streamable-http",
                  "headers": {
                    "Authorization": "Bearer ${__MCPCALL_TEST_TOKEN_NOT_SET__:-fallback-token}"
                  },
                  "roots": ["file://${__MCPCALL_TEST_ROOT_NOT_SET__:-/repo}"]
                }
              }
            }"#,
        )
        .unwrap();

        let options = config
            .server("linear")
            .unwrap()
            .to_transport_options(ConfigOverlay::default())
            .unwrap();

        assert_eq!(options.roots, vec!["file:///repo"]);
        match options.endpoint {
            Endpoint::Http { url, headers, .. } => {
                assert_eq!(url, "https://example.com/mcp");
                assert_eq!(headers[0].value, "Bearer fallback-token");
            }
            _ => panic!("expected http endpoint"),
        }
    }

    #[test]
    fn normalizes_config_root_paths_to_file_uris() {
        let root = normalize_root("relative-root").unwrap();
        assert!(root.starts_with("file://"));
        assert!(root.ends_with("/relative-root"));
    }
}
