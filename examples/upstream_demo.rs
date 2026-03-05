use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Request, Response, Server};
use serde_json::json;
use std::convert::Infallible;
use std::net::SocketAddr;

async fn handler(req: Request<Body>) -> Result<Response<Body>, Infallible> {
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

#[tokio::main]
async fn main() {
    let addr: SocketAddr = std::env::var("UPSTREAM_ADDR")
        .unwrap_or_else(|_| "127.0.0.1:8080".to_string())
        .parse()
        .expect("invalid UPSTREAM_ADDR");

    let make_svc =
        make_service_fn(|_| async { Ok::<_, Infallible>(service_fn(|req| handler(req))) });

    eprintln!("upstream demo listening on {}", addr);
    Server::bind(&addr)
        .serve(make_svc)
        .await
        .expect("server failed");
}
