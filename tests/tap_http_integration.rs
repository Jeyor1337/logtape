use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Request, Response, Server};
use serde_json::{json, Value};
use std::convert::Infallible;
use std::io::Read;
use std::net::{SocketAddr, TcpListener};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};
use tokio::sync::oneshot;

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

async fn upstream_handler(req: Request<Body>) -> Result<Response<Body>, Infallible> {
    let method = req.method().to_string();
    let path = req.uri().path().to_string();
    let query = req.uri().query().unwrap_or("").to_string();
    let traceparent = req
        .headers()
        .get("traceparent")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    let body = hyper::body::to_bytes(req.into_body())
        .await
        .unwrap_or_default();

    let payload = json!({
        "ok": true,
        "method": method,
        "path": path,
        "query": query,
        "traceparent": traceparent,
        "body": String::from_utf8_lossy(&body),
    });

    Ok(Response::builder()
        .status(200)
        .header("content-type", "application/json")
        .header("x-upstream-traceparent", traceparent)
        .body(Body::from(payload.to_string()))
        .unwrap())
}

fn start_upstream(port: u16) -> (oneshot::Sender<()>, thread::JoinHandle<()>) {
    let (tx, rx) = oneshot::channel::<()>();
    let handle = thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async move {
            let addr = SocketAddr::from(([127, 0, 0, 1], port));
            let make_svc =
                make_service_fn(|_| async { Ok::<_, Infallible>(service_fn(upstream_handler)) });
            let server = Server::bind(&addr).serve(make_svc);
            let graceful = server.with_graceful_shutdown(async {
                let _ = rx.await;
            });
            let _ = graceful.await;
        });
    });
    (tx, handle)
}

fn start_proxy(listen: u16, upstream: u16, out_file: &str) -> Child {
    Command::new(assert_cmd::cargo::cargo_bin!("logtape"))
        .args([
            "tap",
            "http",
            "--listen",
            &format!("127.0.0.1:{}", listen),
            "--upstream",
            &format!("http://127.0.0.1:{}", upstream),
            "--out",
            out_file,
            "--body-max",
            "8",
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap()
}

fn wait_until_ready(url: &str, retries: usize, delay: Duration) {
    let client = reqwest::blocking::Client::new();
    for _ in 0..retries {
        if client
            .get(url)
            .send()
            .map(|r| r.status().is_success())
            .unwrap_or(false)
        {
            return;
        }
        thread::sleep(delay);
    }
    panic!("service not ready: {}", url);
}

fn wait_last_line(path: &Path, timeout: Duration) -> String {
    let deadline = Instant::now() + timeout;
    loop {
        if let Ok(mut f) = std::fs::File::open(path) {
            let mut s = String::new();
            if f.read_to_string(&mut s).is_ok() {
                if let Some(last) = s.lines().last() {
                    if !last.trim().is_empty() {
                        return last.to_string();
                    }
                }
            }
        }
        if Instant::now() > deadline {
            panic!("no event line produced at {}", path.display());
        }
        thread::sleep(Duration::from_millis(120));
    }
}

#[test]
fn tap_http_records_redaction_trace_and_body_fields() {
    let upstream_port = free_port();
    let proxy_port = free_port();
    let dir = tempfile::tempdir().unwrap();
    let out_path = dir.path().join("tape.jsonl");

    let (shutdown_tx, upstream_thread) = start_upstream(upstream_port);

    let mut proxy = start_proxy(proxy_port, upstream_port, out_path.to_str().unwrap());
    wait_until_ready(
        &format!("http://127.0.0.1:{}/ready", proxy_port),
        120,
        Duration::from_millis(100),
    );

    let client = reqwest::blocking::Client::new();
    let resp = client
        .post(format!(
            "http://127.0.0.1:{}/hello?token=abc&x=1",
            proxy_port
        ))
        .header("authorization", "Bearer secret")
        .header(
            "traceparent",
            "00-11111111111111111111111111111111-2222222222222222-01",
        )
        .body("abcdef123456")
        .send()
        .unwrap();
    assert!(resp.status().is_success());

    let line = wait_last_line(&out_path, Duration::from_secs(8));
    let v: Value = serde_json::from_str(&line).unwrap();

    let _ = proxy.kill();
    let _ = proxy.wait();
    let _ = shutdown_tx.send(());
    let _ = upstream_thread.join();

    assert_eq!(v["v"], "0.1");
    assert_eq!(v["kind"], "http");
    assert_eq!(v["name"], "http.server");
    assert_eq!(v["trace_id"], "11111111111111111111111111111111");

    let span_id = v["span_id"].as_str().unwrap();
    assert_eq!(span_id.len(), 16);
    assert_ne!(span_id, "2222222222222222");

    assert_eq!(v["attrs"]["http.method"], "POST");
    assert_eq!(v["attrs"]["url.path"], "/hello");
    assert_eq!(v["attrs"]["url.query"], "token=[REDACTED]&x=1");

    assert_eq!(
        v["attrs"]["http.request.headers"]["authorization"],
        "[REDACTED]"
    );

    assert!(v["attrs"]["server.duration_ms"].as_f64().unwrap() >= 0.0);

    assert_eq!(v["attrs"]["http.request.body.size"], 12);
    assert_eq!(v["attrs"]["http.request.body.truncated"], true);
    assert_eq!(v["attrs"]["http.request.body.preview"], "abcdef12");
    assert_eq!(v["attrs"]["http.request.body.encoding"], "utf-8");
    assert_eq!(
        v["attrs"]["http.request.body.sha256"]
            .as_str()
            .unwrap()
            .len(),
        64
    );

    assert_eq!(
        v["attrs"]["http.response.status_code"].as_u64().unwrap(),
        200
    );
    assert!(v["attrs"]["http.response.body.size"].as_u64().unwrap() > 0);

    let tp = v["attrs"]["http.response.headers"]["x-upstream-traceparent"]
        .as_str()
        .unwrap();
    assert_eq!(
        tp,
        format!("00-11111111111111111111111111111111-{}-01", span_id)
    );
}
