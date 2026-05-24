use std::path::PathBuf;

use anyhow::{Context, Result, bail};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransportOptions {
    pub endpoint: Endpoint,
    pub timeout_secs: u64,
    pub roots: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Endpoint {
    Http {
        url: String,
        bearer: Option<String>,
        headers: Vec<KeyValue>,
    },
    Sse {
        url: String,
        bearer: Option<String>,
        headers: Vec<KeyValue>,
    },
    Stdio {
        command: String,
        cwd: Option<PathBuf>,
        env: Vec<KeyValue>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyValue {
    pub key: String,
    pub value: String,
}

pub fn parse_key_values(values: &[String], flag: &str) -> Result<Vec<KeyValue>> {
    values
        .iter()
        .map(|value| {
            let (key, raw) = value
                .split_once('=')
                .with_context(|| format!("{flag} expects KEY=VALUE, got {value:?}"))?;
            if key.trim().is_empty() {
                bail!("{flag} key cannot be empty");
            }
            Ok(KeyValue {
                key: key.trim().to_owned(),
                value: raw.to_owned(),
            })
        })
        .collect()
}

pub fn key_values_from_map(values: &std::collections::BTreeMap<String, String>) -> Vec<KeyValue> {
    values
        .iter()
        .map(|(key, value)| KeyValue {
            key: key.clone(),
            value: value.clone(),
        })
        .collect()
}
