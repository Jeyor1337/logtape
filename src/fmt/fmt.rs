use serde_json::Value;
use std::collections::BTreeMap;
use std::io::BufRead;

fn logfmt_escape(s: &str) -> String {
    if s.is_empty() || s.contains(' ') || s.contains('"') || s.contains('\\') || s.contains('=') {
        let escaped = s.replace('\\', "\\\\").replace('"', "\\\"");
        format!("\"{}\"", escaped)
    } else {
        s.to_string()
    }
}

fn flatten_value(prefix: &str, value: &Value, parts: &mut Vec<String>) {
    match value {
        Value::Object(map) => {
            for (k, v) in map {
                let key = if prefix.is_empty() {
                    k.clone()
                } else {
                    format!("{}.{}", prefix, k)
                };
                flatten_value(&key, v, parts);
            }
        }
        Value::String(s) => {
            parts.push(format!("{}={}", prefix, logfmt_escape(s)));
        }
        Value::Number(n) => {
            parts.push(format!("{}={}", prefix, n));
        }
        Value::Bool(b) => {
            parts.push(format!("{}={}", prefix, b));
        }
        Value::Null => {
            parts.push(format!("{}=null", prefix));
        }
        Value::Array(_) => {
            let json_str = serde_json::to_string(value).unwrap_or_default();
            parts.push(format!("{}={}", prefix, logfmt_escape(&json_str)));
        }
    }
}

pub fn jsonl_to_logfmt<R: BufRead>(reader: R) -> Vec<String> {
    let mut lines = Vec::new();
    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => continue,
        };
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let obj: BTreeMap<String, Value> = match serde_json::from_str(trimmed) {
            Ok(o) => o,
            Err(_) => continue,
        };

        let mut parts = Vec::new();
        for (k, v) in &obj {
            if k == "attrs" {
                if let Value::Object(attrs) = v {
                    for (ak, av) in attrs {
                        flatten_value(&format!("a.{}", ak), av, &mut parts);
                    }
                }
            } else {
                flatten_value(k, v, &mut parts);
            }
        }
        lines.push(parts.join(" "));
    }
    lines
}

fn parse_logfmt_value_with_key(key: &str, s: &str) -> Value {
    let force_string_keys = [
        "v",
        "ts",
        "kind",
        "name",
        "trace_id",
        "span_id",
        "level",
        "service",
        "service_version",
    ];
    let force_string_prefixes = [
        "a.http.method",
        "a.url.path",
        "a.url.scheme",
        "a.url.query",
        "a.network.protocol.version",
        "a.http.request.body.sha256",
        "a.http.response.body.sha256",
        "a.http.request.body.preview",
        "a.http.response.body.preview",
        "a.http.request.body.encoding",
        "a.http.response.body.encoding",
    ];

    if force_string_keys.contains(&key) || force_string_prefixes.iter().any(|p| key.starts_with(p))
    {
        return Value::String(s.to_string());
    }

    if s == "true" {
        return Value::Bool(true);
    }
    if s == "false" {
        return Value::Bool(false);
    }
    if s == "null" {
        return Value::Null;
    }
    if let Ok(n) = s.parse::<i64>() {
        return Value::Number(n.into());
    }
    if let Ok(n) = s.parse::<f64>() {
        if let Some(num) = serde_json::Number::from_f64(n) {
            return Value::Number(num);
        }
    }
    if (s.starts_with('[') && s.ends_with(']')) || (s.starts_with('{') && s.ends_with('}')) {
        if let Ok(v) = serde_json::from_str::<Value>(s) {
            return v;
        }
    }
    Value::String(s.to_string())
}

pub fn logfmt_to_jsonl(input: &str) -> Vec<String> {
    let mut results = Vec::new();
    for line in input.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let mut top: BTreeMap<String, Value> = BTreeMap::new();
        let mut attrs = serde_json::Map::new();

        let pairs = parse_logfmt_pairs(trimmed);
        for (key, val) in pairs {
            let value = parse_logfmt_value_with_key(&key, &val);
            if let Some(attr_key) = key.strip_prefix("a.") {
                attrs.insert(attr_key.to_string(), value);
            } else {
                top.insert(key, value);
            }
        }

        if !attrs.is_empty() {
            top.insert("attrs".to_string(), Value::Object(attrs));
        }

        if let Ok(json) = serde_json::to_string(&top) {
            results.push(json);
        }
    }
    results
}

fn parse_logfmt_pairs(s: &str) -> Vec<(String, String)> {
    let mut pairs = Vec::new();
    let chars: Vec<char> = s.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        while i < len && chars[i] == ' ' {
            i += 1;
        }
        if i >= len {
            break;
        }

        let key_start = i;
        while i < len && chars[i] != '=' && chars[i] != ' ' {
            i += 1;
        }
        let key = chars[key_start..i].iter().collect::<String>();

        if i >= len || chars[i] != '=' {
            continue;
        }
        i += 1;

        let value = if i < len && chars[i] == '"' {
            i += 1;
            let mut val = String::new();
            while i < len && chars[i] != '"' {
                if chars[i] == '\\' && i + 1 < len {
                    i += 1;
                    match chars[i] {
                        '"' => val.push('"'),
                        '\\' => val.push('\\'),
                        _ => {
                            val.push('\\');
                            val.push(chars[i]);
                        }
                    }
                } else {
                    val.push(chars[i]);
                }
                i += 1;
            }
            if i < len {
                i += 1;
            }
            val
        } else {
            let val_start = i;
            while i < len && chars[i] != ' ' {
                i += 1;
            }
            chars[val_start..i].iter().collect()
        };

        pairs.push((key, value));
    }

    pairs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_logfmt_escape_plain() {
        assert_eq!(logfmt_escape("hello"), "hello");
    }

    #[test]
    fn test_logfmt_escape_space() {
        assert_eq!(logfmt_escape("hello world"), "\"hello world\"");
    }

    #[test]
    fn test_logfmt_escape_quotes() {
        assert_eq!(logfmt_escape("say \"hi\""), "\"say \\\"hi\\\"\"");
    }
}
