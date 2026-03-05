use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub v: String,
    pub ts: String,
    pub kind: String,
    pub name: String,
    pub trace_id: String,
    pub span_id: String,
    pub attrs: BTreeMap<String, Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub level: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service_version: Option<String>,
}

impl Event {
    pub fn new_http(trace_id: String, span_id: String) -> Self {
        let now: DateTime<Utc> = Utc::now();
        Self {
            v: "0.1".to_string(),
            ts: now.format("%Y-%m-%dT%H:%M:%S%.9fZ").to_string(),
            kind: "http".to_string(),
            name: "http.server".to_string(),
            trace_id,
            span_id,
            attrs: BTreeMap::new(),
            level: None,
            service: None,
            service_version: None,
        }
    }

    pub fn set_attr(&mut self, key: &str, value: Value) {
        self.attrs.insert(key.to_string(), value);
    }

    pub fn to_json_line(&self) -> serde_json::Result<String> {
        serde_json::to_string(self)
    }
}
