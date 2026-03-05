use crate::spec::{validate_event, DiagLevel, Diagnostic};
use serde_json::Value;
use std::collections::BTreeMap;
use std::io::BufRead;

pub struct LintResult {
    pub diagnostics: Vec<Diagnostic>,
    pub lines_checked: usize,
}

pub fn lint_reader<R: BufRead>(reader: R) -> LintResult {
    let mut diagnostics = Vec::new();
    let mut lines_checked = 0;

    for (i, line) in reader.lines().enumerate() {
        let line_num = i + 1;
        match line {
            Ok(text) => {
                let text = text.trim();
                if text.is_empty() {
                    continue;
                }
                lines_checked += 1;
                match serde_json::from_str::<BTreeMap<String, Value>>(text) {
                    Ok(obj) => {
                        let mut diags = validate_event(line_num, &obj);
                        diagnostics.append(&mut diags);
                    }
                    Err(e) => {
                        diagnostics.push(Diagnostic {
                            line: line_num,
                            level: DiagLevel::Error,
                            message: format!("invalid JSON: {}", e),
                        });
                    }
                }
            }
            Err(e) => {
                diagnostics.push(Diagnostic {
                    line: line_num,
                    level: DiagLevel::Error,
                    message: format!("read error: {}", e),
                });
            }
        }
    }

    LintResult {
        diagnostics,
        lines_checked,
    }
}

pub fn has_errors(diags: &[Diagnostic], strict: bool) -> bool {
    diags
        .iter()
        .any(|d| d.level == DiagLevel::Error || (strict && d.level == DiagLevel::Warning))
}
