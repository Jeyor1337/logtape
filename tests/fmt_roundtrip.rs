use serde_json::Value;
use std::process::Command;

fn run_logtape(args: &[&str]) -> std::process::Output {
    Command::new(assert_cmd::cargo::cargo_bin!("logtape"))
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn fmt_roundtrip_jsonl_logfmt_jsonl_semantic_equal() {
    let input = r#"{"v":"0.1","ts":"2026-03-04T12:00:00.123456789Z","kind":"http","name":"http.server","trace_id":"0123456789abcdef0123456789abcdef","span_id":"0123456789abcdef","attrs":{"http.method":"POST","url.path":"/v1/demo","server.duration_ms":8.5,"http.response.status_code":201,"nested":{"k":"v"}}}
"#;

    let dir = tempfile::tempdir().unwrap();
    let jsonl_path = dir.path().join("in.jsonl");
    let logfmt_path = dir.path().join("out.logfmt");
    std::fs::write(&jsonl_path, input).unwrap();

    let out1 = run_logtape(&["fmt", "--to", "logfmt", jsonl_path.to_str().unwrap()]);
    assert!(out1.status.success());
    std::fs::write(&logfmt_path, &out1.stdout).unwrap();

    let out2 = run_logtape(&["fmt", "--to", "jsonl", logfmt_path.to_str().unwrap()]);
    assert!(out2.status.success());

    let original: Value = serde_json::from_str(input.trim()).unwrap();
    let restored: Value = serde_json::from_slice(&out2.stdout).unwrap();

    assert_eq!(original["v"], restored["v"]);
    assert_eq!(original["kind"], restored["kind"]);
    assert_eq!(original["name"], restored["name"]);
    assert_eq!(original["trace_id"], restored["trace_id"]);
    assert_eq!(original["span_id"], restored["span_id"]);
    assert_eq!(
        original["attrs"]["http.method"],
        restored["attrs"]["http.method"]
    );
    assert_eq!(original["attrs"]["url.path"], restored["attrs"]["url.path"]);
}
