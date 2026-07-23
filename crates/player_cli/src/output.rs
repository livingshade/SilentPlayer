use serde_json::{Map, Value};

use crate::error::{CliError, CliResult};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OutputMode {
    Table,
    Json,
}

impl OutputMode {
    pub fn parse(value: &str) -> CliResult<Self> {
        match value {
            "table" => Ok(Self::Table),
            "json" => Ok(Self::Json),
            _ => Err(CliError::usage("--output must be either `table` or `json`")),
        }
    }
}

pub fn emit(value: &Value, mode: OutputMode, quiet: bool) -> CliResult<()> {
    if quiet {
        return Ok(());
    }
    match mode {
        OutputMode::Json => {
            println!("{}", serde_json::to_string_pretty(value)?);
        }
        OutputMode::Table => print_human(value, 0),
    }
    Ok(())
}

fn print_human(value: &Value, indent: usize) {
    match value {
        Value::Null => println!("null"),
        Value::Bool(value) => println!("{value}"),
        Value::Number(value) => println!("{value}"),
        Value::String(value) => println!("{value}"),
        Value::Array(values) => {
            if values.is_empty() {
                println!("No results.");
                return;
            }
            for value in values {
                if let Some(row) = compact_row(value) {
                    println!("{row}");
                } else {
                    print_human(value, indent);
                }
            }
        }
        Value::Object(values) if values.is_empty() => println!("OK"),
        Value::Object(values) => print_object(values, indent),
    }
}

fn print_object(values: &Map<String, Value>, indent: usize) {
    for (key, value) in values {
        let padding = " ".repeat(indent);
        match value {
            Value::Array(items) => {
                println!("{padding}{}:", display_key(key));
                if items.is_empty() {
                    println!("{padding}  (none)");
                } else {
                    for item in items {
                        if let Some(row) = compact_row(item) {
                            println!("{padding}  {row}");
                        } else {
                            print_human(item, indent + 2);
                        }
                    }
                }
            }
            Value::Object(object) => {
                println!("{padding}{}:", display_key(key));
                print_object(object, indent + 2);
            }
            _ => println!("{padding}{}: {}", display_key(key), scalar_text(value)),
        }
    }
}

fn compact_row(value: &Value) -> Option<String> {
    let object = value.as_object()?;
    if let (Some(title), Some(path)) = (
        object.get("title").and_then(Value::as_str),
        object.get("path").and_then(Value::as_str),
    ) {
        let mut fields = Vec::new();
        if let Some(view) = object
            .get("view_id")
            .and_then(Value::as_str)
            .or_else(|| object.get("id").and_then(Value::as_str))
        {
            fields.push(view);
        }
        fields.push(title);
        if let Some(artist) = object
            .get("artist")
            .and_then(Value::as_str)
            .filter(|artist| !artist.is_empty())
        {
            fields.push(artist);
        }
        fields.push(path);
        return Some(fields.join(" | "));
    }
    if let (Some(name), Some(count)) = (
        object.get("name").and_then(Value::as_str),
        object.get("track_count").and_then(Value::as_u64),
    ) {
        return Some(match object.get("id") {
            Some(id) => format!("{} | {name} | {count} tracks", scalar_text(id)),
            None => format!("{name} | {count} tracks"),
        });
    }
    None
}

fn display_key(value: &str) -> String {
    value.replace('_', " ")
}

fn scalar_text(value: &Value) -> String {
    match value {
        Value::Null => "-".to_owned(),
        Value::Bool(value) => value.to_string(),
        Value::Number(value) => value.to_string(),
        Value::String(value) => value.clone(),
        _ => value.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_mode_has_no_compatibility_aliases() {
        assert_eq!(OutputMode::parse("table").unwrap(), OutputMode::Table);
        assert_eq!(OutputMode::parse("json").unwrap(), OutputMode::Json);
        assert!(OutputMode::parse("human").is_err());
        assert!(OutputMode::parse("text").is_err());
        assert!(OutputMode::parse("JSON").is_err());
    }
}
