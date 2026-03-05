use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub line: usize,
    pub level: DiagLevel,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DiagLevel {
    Error,
    Warning,
}

impl std::fmt::Display for DiagLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DiagLevel::Error => write!(f, "error"),
            DiagLevel::Warning => write!(f, "warning"),
        }
    }
}

fn is_hex(s: &str, len: usize) -> bool {
    s.len() == len
        && s.chars()
            .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c))
}

pub fn validate_event(line_num: usize, obj: &BTreeMap<String, Value>) -> Vec<Diagnostic> {
    let mut diags = Vec::new();

    macro_rules! require_str {
        ($key:expr) => {
            match obj.get($key) {
                Some(Value::String(s)) => Some(s.as_str()),
                Some(_) => {
                    diags.push(Diagnostic {
                        line: line_num,
                        level: DiagLevel::Error,
                        message: format!("field '{}' must be a string", $key),
                    });
                    None
                }
                None => {
                    diags.push(Diagnostic {
                        line: line_num,
                        level: DiagLevel::Error,
                        message: format!("missing required field '{}'", $key),
                    });
                    None
                }
            }
        };
    }

    if let Some(v) = require_str!("v") {
        if v != "0.1" {
            diags.push(Diagnostic {
                line: line_num,
                level: DiagLevel::Warning,
                message: format!("unknown version '{}'", v),
            });
        }
    }

    if let Some(ts) = require_str!("ts") {
        if chrono::DateTime::parse_from_rfc3339(ts).is_err()
            && chrono::NaiveDateTime::parse_from_str(ts, "%Y-%m-%dT%H:%M:%S%.fZ").is_err()
        {
            diags.push(Diagnostic {
                line: line_num,
                level: DiagLevel::Error,
                message: "field 'ts' is not valid RFC 3339".to_string(),
            });
        }
    }

    require_str!("kind");
    require_str!("name");

    if let Some(tid) = require_str!("trace_id") {
        if !is_hex(tid, 32) {
            diags.push(Diagnostic {
                line: line_num,
                level: DiagLevel::Error,
                message: "trace_id must be 32 lowercase hex chars".to_string(),
            });
        }
    }

    if let Some(sid) = require_str!("span_id") {
        if !is_hex(sid, 16) {
            diags.push(Diagnostic {
                line: line_num,
                level: DiagLevel::Error,
                message: "span_id must be 16 lowercase hex chars".to_string(),
            });
        }
    }

    match obj.get("attrs") {
        Some(Value::Object(_)) => {}
        Some(_) => {
            diags.push(Diagnostic {
                line: line_num,
                level: DiagLevel::Error,
                message: "field 'attrs' must be an object".to_string(),
            });
        }
        None => {
            diags.push(Diagnostic {
                line: line_num,
                level: DiagLevel::Error,
                message: "missing required field 'attrs'".to_string(),
            });
        }
    }

    let kind = obj.get("kind").and_then(|v| v.as_str());
    if kind == Some("http") {
        if let Some(Value::Object(attrs)) = obj.get("attrs") {
            if !attrs.contains_key("http.method") {
                diags.push(Diagnostic {
                    line: line_num,
                    level: DiagLevel::Error,
                    message: "http event missing attrs 'http.method'".to_string(),
                });
            }
            if !attrs.contains_key("url.path") {
                diags.push(Diagnostic {
                    line: line_num,
                    level: DiagLevel::Error,
                    message: "http event missing attrs 'url.path'".to_string(),
                });
            }
            if !attrs.contains_key("server.duration_ms") {
                diags.push(Diagnostic {
                    line: line_num,
                    level: DiagLevel::Error,
                    message: "http event missing attrs 'server.duration_ms'".to_string(),
                });
            }

            let has_error = attrs.contains_key("error.type") || attrs.contains_key("error.message");
            if !has_error && !attrs.contains_key("http.response.status_code") {
                diags.push(Diagnostic {
                    line: line_num,
                    level: DiagLevel::Error,
                    message:
                        "http event missing 'http.response.status_code' (required when no error)"
                            .to_string(),
                });
            }
        }
    }

    diags
}
