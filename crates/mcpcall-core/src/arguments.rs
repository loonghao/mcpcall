use std::{fs, io::Read};

use anyhow::{Context, Result, bail};
use serde_json::{Map, Value};

pub type JsonObject = Map<String, Value>;

#[derive(Debug, Clone, PartialEq)]
pub struct ParsedArguments {
    pub name: String,
    pub arguments: JsonObject,
}

pub fn parse_call_arguments(
    target: &str,
    args_json: Option<&str>,
    repeated_args: &[String],
    positional_pairs: &[String],
) -> Result<ParsedArguments> {
    let mut parsed = parse_target(target)?;
    merge_arguments(
        &mut parsed.arguments,
        args_json,
        repeated_args,
        positional_pairs,
    )?;
    Ok(parsed)
}

pub fn parse_named_arguments(
    name: &str,
    args_json: Option<&str>,
    repeated_args: &[String],
    positional_pairs: &[String],
) -> Result<ParsedArguments> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        bail!("name cannot be empty");
    }

    let mut parsed = ParsedArguments {
        name: trimmed.to_owned(),
        arguments: JsonObject::new(),
    };
    merge_arguments(
        &mut parsed.arguments,
        args_json,
        repeated_args,
        positional_pairs,
    )?;
    Ok(parsed)
}

fn merge_arguments(
    arguments: &mut JsonObject,
    args_json: Option<&str>,
    repeated_args: &[String],
    positional_pairs: &[String],
) -> Result<()> {
    if let Some(args_json) = args_json {
        arguments.extend(read_json_object(args_json)?);
    }

    for pair in repeated_args.iter().chain(positional_pairs.iter()) {
        let (key, value) = parse_pair(pair)?;
        arguments.insert(key, parse_scalar_or_json(value)?);
    }

    Ok(())
}

fn parse_target(target: &str) -> Result<ParsedArguments> {
    let trimmed = target.trim();
    let Some(open) = trimmed.find('(') else {
        if trimmed.is_empty() {
            bail!("tool name cannot be empty");
        }
        return Ok(ParsedArguments {
            name: trimmed.to_owned(),
            arguments: JsonObject::new(),
        });
    };

    if !trimmed.ends_with(')') {
        bail!("function-style target must end with ')'");
    }

    let tool_name = trimmed[..open].trim();
    if tool_name.is_empty() {
        bail!("tool name cannot be empty");
    }

    let inner = trimmed[open + 1..trimmed.len() - 1].trim();
    let arguments = if inner.is_empty() {
        JsonObject::new()
    } else if inner.starts_with('{') {
        ensure_json_object(serde_json::from_str(inner)?, "function-style arguments")?
    } else {
        parse_comma_arguments(inner)?
    };

    Ok(ParsedArguments {
        name: tool_name.to_owned(),
        arguments,
    })
}

fn read_json_object(raw: &str) -> Result<JsonObject> {
    let json_text = if raw == "-" {
        let mut buffer = String::new();
        std::io::stdin()
            .read_to_string(&mut buffer)
            .context("read --args JSON from stdin")?;
        buffer
    } else if let Some(path) = raw.strip_prefix('@') {
        fs::read_to_string(path).with_context(|| format!("read JSON args file {path}"))?
    } else {
        raw.to_owned()
    };
    ensure_json_object(serde_json::from_str(&json_text)?, "--args")
}

fn parse_comma_arguments(input: &str) -> Result<JsonObject> {
    let mut map = JsonObject::new();
    for part in split_top_level(input, ',')? {
        if part.trim().is_empty() {
            continue;
        }
        let (key, value) = parse_pair(&part)?;
        map.insert(key, parse_scalar_or_json(value)?);
    }
    Ok(map)
}

fn parse_pair(input: &str) -> Result<(String, &str)> {
    let trimmed = input.trim();
    let Some(index) = find_pair_separator(trimmed) else {
        bail!("argument {input:?} must be KEY=VALUE or KEY:VALUE");
    };

    let key = trimmed[..index].trim();
    if key.is_empty() {
        bail!("argument key cannot be empty");
    }
    if !key
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' || ch == '.')
    {
        bail!("argument key {key:?} contains unsupported characters");
    }

    Ok((key.to_owned(), trimmed[index + 1..].trim()))
}

fn parse_scalar_or_json(raw: &str) -> Result<Value> {
    if let Some(path) = raw.strip_prefix("@@") {
        return Ok(Value::String(format!("@{path}")));
    }
    if let Some(path) = raw.strip_prefix('@') {
        return Ok(Value::String(
            fs::read_to_string(path).with_context(|| format!("read argument file {path}"))?,
        ));
    }

    if is_single_quoted(raw) {
        return Ok(Value::String(raw[1..raw.len() - 1].to_owned()));
    }

    Ok(serde_json::from_str(raw).unwrap_or_else(|_| Value::String(raw.to_owned())))
}

fn is_single_quoted(raw: &str) -> bool {
    raw.len() >= 2 && raw.starts_with('\'') && raw.ends_with('\'')
}

fn find_pair_separator(input: &str) -> Option<usize> {
    let mut depth = 0usize;
    let mut quote = None;
    let mut escape = false;

    for (index, ch) in input.char_indices() {
        if let Some(q) = quote {
            if escape {
                escape = false;
            } else if ch == '\\' {
                escape = true;
            } else if ch == q {
                quote = None;
            }
            continue;
        }

        match ch {
            '"' | '\'' => quote = Some(ch),
            '{' | '[' | '(' => depth += 1,
            '}' | ']' | ')' => depth = depth.saturating_sub(1),
            '=' | ':' if depth == 0 => return Some(index),
            _ => {}
        }
    }

    None
}

fn split_top_level(input: &str, separator: char) -> Result<Vec<String>> {
    let mut result = Vec::new();
    let mut start = 0usize;
    let mut depth = 0usize;
    let mut quote = None;
    let mut escape = false;

    for (index, ch) in input.char_indices() {
        if let Some(q) = quote {
            if escape {
                escape = false;
            } else if ch == '\\' {
                escape = true;
            } else if ch == q {
                quote = None;
            }
            continue;
        }

        match ch {
            '"' | '\'' => quote = Some(ch),
            '{' | '[' | '(' => depth += 1,
            '}' | ']' | ')' => {
                if depth == 0 {
                    bail!("unbalanced closing delimiter in arguments");
                }
                depth -= 1;
            }
            ch if ch == separator && depth == 0 => {
                result.push(input[start..index].to_owned());
                start = index + ch.len_utf8();
            }
            _ => {}
        }
    }

    if quote.is_some() {
        bail!("unterminated quote in arguments");
    }
    if depth != 0 {
        bail!("unbalanced delimiter in arguments");
    }

    result.push(input[start..].to_owned());
    Ok(result)
}

fn ensure_json_object(value: Value, source: &str) -> Result<JsonObject> {
    match value {
        Value::Object(map) => Ok(map),
        _ => bail!("{source} must be a JSON object"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_key_value_pairs() {
        let parsed = parse_call_arguments(
            "create_sphere",
            None,
            &["radius=2.5".to_owned()],
            &["name='hero sphere'".to_owned(), "visible=true".to_owned()],
        )
        .unwrap();

        assert_eq!(parsed.name, "create_sphere");
        assert_eq!(parsed.arguments["radius"], json!(2.5));
        assert_eq!(parsed.arguments["name"], json!("hero sphere"));
        assert_eq!(parsed.arguments["visible"], json!(true));
    }

    #[test]
    fn parses_function_style_arguments() {
        let parsed = parse_call_arguments(
            r#"maya_create(name: "cube", opts: {"size": 4})"#,
            None,
            &[],
            &[],
        )
        .unwrap();

        assert_eq!(parsed.name, "maya_create");
        assert_eq!(parsed.arguments["name"], json!("cube"));
        assert_eq!(parsed.arguments["opts"], json!({"size": 4}));
    }

    #[test]
    fn merges_json_and_pairs() {
        let parsed =
            parse_call_arguments("tool", Some(r#"{"a":1,"b":2}"#), &["b=3".to_owned()], &[])
                .unwrap();

        assert_eq!(parsed.arguments["a"], json!(1));
        assert_eq!(parsed.arguments["b"], json!(3));
    }

    #[test]
    fn parses_named_arguments_without_function_syntax() {
        let parsed = parse_named_arguments(
            "review_prompt",
            Some(r#"{"language":"rust"}"#),
            &["strict=true".to_owned()],
            &[],
        )
        .unwrap();

        assert_eq!(parsed.name, "review_prompt");
        assert_eq!(parsed.arguments["language"], json!("rust"));
        assert_eq!(parsed.arguments["strict"], json!(true));
    }
}
