use std::process::Command;

fn run_logtape(args: &[&str]) -> std::process::Output {
    Command::new(assert_cmd::cargo::cargo_bin!("logtape"))
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn lint_good_file_ok() {
    let out = run_logtape(&["lint", "tests/fixtures/good.jsonl"]);
    assert!(out.status.success());
}

#[test]
fn lint_bad_file_fails() {
    let out = run_logtape(&["lint", "tests/fixtures/bad.jsonl"]);
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("trace_id must be 32 lowercase hex chars"));
}

#[test]
fn lint_strict_treats_warning_as_error() {
    let path = "tests/fixtures/version_warning.jsonl";
    std::fs::write(path, "{\"v\":\"0.2\",\"ts\":\"2026-03-04T12:00:00Z\",\"kind\":\"http\",\"name\":\"http.server\",\"trace_id\":\"0123456789abcdef0123456789abcdef\",\"span_id\":\"0123456789abcdef\",\"attrs\":{\"http.method\":\"GET\",\"url.path\":\"/\",\"server.duration_ms\":1.2,\"http.response.status_code\":200}}\n").unwrap();

    let out = run_logtape(&["lint", path, "--strict"]);
    assert!(!out.status.success());

    let _ = std::fs::remove_file(path);
}
