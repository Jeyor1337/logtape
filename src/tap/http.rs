use crate::event::Event;
use base64::Engine;
use hyper::client::HttpConnector;
use hyper::header::HeaderMap;
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Client, Request, Response, Server, StatusCode, Uri};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::io::{BufWriter, Write};
use std::net::SocketAddr;
use std::sync::{mpsc, Arc};

pub struct TapConfig {
    pub listen: SocketAddr,
    pub upstream: String,
    pub out: OutputTarget,
    pub service: Option<String>,
    pub service_version: Option<String>,
    pub body_max: usize,
    pub redact_headers: HashSet<String>,
    pub redact_query: HashSet<String>,
}

pub enum OutputTarget {
    Stdout,
    File(String),
}

struct ProxyState {
    config: TapConfig,
    client: Client<HttpConnector, Body>,
    log_tx: mpsc::Sender<String>,
}

fn generate_hex(len: usize) -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    (0..len)
        .map(|_| format!("{:x}", rng.gen::<u8>() % 16))
        .collect()
}

fn parse_traceparent(val: &str) -> Option<(String, String)> {
    let parts: Vec<&str> = val.split('-').collect();
    if parts.len() == 4
        && parts[0] == "00"
        && parts[1].len() == 32
        && parts[2].len() == 16
        && parts[1].chars().all(|c| c.is_ascii_hexdigit())
        && parts[2].chars().all(|c| c.is_ascii_hexdigit())
    {
        Some((parts[1].to_lowercase(), parts[2].to_lowercase()))
    } else {
        None
    }
}

fn redact_headers(headers: &HeaderMap, redact_set: &HashSet<String>) -> Value {
    let mut map = serde_json::Map::new();
    for (name, value) in headers {
        let key = name.as_str().to_lowercase();
        let val = if redact_set.contains(&key) {
            "[REDACTED]".to_string()
        } else {
            value.to_str().unwrap_or("[non-utf8]").to_string()
        };
        map.insert(key, Value::String(val));
    }
    Value::Object(map)
}

fn redact_query(query: Option<&str>, redact_set: &HashSet<String>) -> Option<String> {
    let q = query?;
    if q.is_empty() {
        return None;
    }
    let pairs: Vec<String> = q
        .split('&')
        .map(|pair| {
            if let Some((k, v)) = pair.split_once('=') {
                if redact_set.contains(&k.to_lowercase()) {
                    format!("{}=[REDACTED]", k)
                } else {
                    format!("{}={}", k, v)
                }
            } else if redact_set.contains(&pair.to_lowercase()) {
                format!("{}=[REDACTED]", pair)
            } else {
                pair.to_string()
            }
        })
        .collect();
    Some(pairs.join("&"))
}

fn body_attrs(prefix: &str, body: &[u8], max_preview: usize) -> Vec<(String, Value)> {
    let mut attrs = Vec::new();
    let size = body.len();
    attrs.push((format!("{}.body.size", prefix), Value::Number(size.into())));
    attrs.push((
        format!("{}.body.truncated", prefix),
        Value::Bool(size > max_preview),
    ));

    let mut hasher = Sha256::new();
    hasher.update(body);
    let hash = format!("{:x}", hasher.finalize());
    attrs.push((format!("{}.body.sha256", prefix), Value::String(hash)));

    let preview_bytes = &body[..size.min(max_preview)];
    let (preview, encoding) = match std::str::from_utf8(preview_bytes) {
        Ok(s) => (s.to_string(), "utf-8"),
        Err(_) => (
            base64::engine::general_purpose::STANDARD.encode(preview_bytes),
            "base64",
        ),
    };
    attrs.push((format!("{}.body.preview", prefix), Value::String(preview)));
    attrs.push((
        format!("{}.body.encoding", prefix),
        Value::String(encoding.to_string()),
    ));

    attrs
}

fn http_version_string(version: hyper::Version) -> String {
    match version {
        hyper::Version::HTTP_09 => "0.9".to_string(),
        hyper::Version::HTTP_10 => "1.0".to_string(),
        hyper::Version::HTTP_11 => "1.1".to_string(),
        hyper::Version::HTTP_2 => "2".to_string(),
        _ => "unknown".to_string(),
    }
}

fn bad_gateway_response() -> Response<Body> {
    let mut response = Response::new(Body::from("Bad Gateway"));
    *response.status_mut() = StatusCode::BAD_GATEWAY;
    response
}

fn attach_proxy_error(event: &mut Event, message: String, body_max: usize) {
    event.level = Some("error".to_string());
    event.set_attr("error.type", Value::String("proxy_error".to_string()));
    event.set_attr("error.message", Value::String(message));
    event.set_attr(
        "http.response.headers",
        Value::Object(serde_json::Map::new()),
    );
    for (k, v) in body_attrs("http.response", b"", body_max) {
        event.set_attr(&k, v);
    }
}

async fn proxy_handler(
    req: Request<Body>,
    state: Arc<ProxyState>,
) -> Result<Response<Body>, hyper::Error> {
    let start = std::time::Instant::now();

    let (parts, body) = req.into_parts();
    let method = parts.method.clone();
    let version = http_version_string(parts.version);
    let req_headers = parts.headers.clone();
    let original_uri = parts.uri.clone();

    let (trace_id, _inbound_span_id) = req_headers
        .get("traceparent")
        .and_then(|v| v.to_str().ok())
        .and_then(parse_traceparent)
        .unwrap_or_else(|| (generate_hex(32), generate_hex(16)));
    let span_id = generate_hex(16);

    let path = original_uri.path().to_string();
    let query_raw = original_uri.query().map(|s| s.to_string());
    let scheme = {
        let parsed = url::Url::parse(&state.config.upstream).ok();
        parsed
            .as_ref()
            .map(|u| u.scheme().to_string())
            .unwrap_or_else(|| "http".to_string())
    };

    let req_body_bytes = hyper::body::to_bytes(body).await.unwrap_or_default();

    let mut event = Event::new_http(trace_id.clone(), span_id.clone());
    if let Some(ref svc) = state.config.service {
        event.service = Some(svc.clone());
    }
    if let Some(ref ver) = state.config.service_version {
        event.service_version = Some(ver.clone());
    }

    event.set_attr("http.method", Value::String(method.to_string()));
    event.set_attr("url.scheme", Value::String(scheme));
    event.set_attr("url.path", Value::String(path));
    if let Some(redacted) = redact_query(query_raw.as_deref(), &state.config.redact_query) {
        event.set_attr("url.query", Value::String(redacted));
    }
    event.set_attr("network.protocol.version", Value::String(version));
    event.set_attr(
        "http.request.headers",
        redact_headers(&req_headers, &state.config.redact_headers),
    );
    for (k, v) in body_attrs("http.request", &req_body_bytes, state.config.body_max) {
        event.set_attr(&k, v);
    }

    let upstream_uri: Uri = match format!(
        "{}{}{}",
        state.config.upstream.trim_end_matches('/'),
        original_uri.path(),
        query_raw
            .as_ref()
            .map(|q| format!("?{}", q))
            .unwrap_or_default()
    )
    .parse()
    {
        Ok(uri) => uri,
        Err(_) => match state.config.upstream.parse() {
            Ok(uri) => uri,
            Err(e) => {
                attach_proxy_error(
                    &mut event,
                    format!("invalid upstream url: {}", e),
                    state.config.body_max,
                );
                let elapsed = start.elapsed().as_secs_f64() * 1000.0;
                event.set_attr(
                    "server.duration_ms",
                    Value::Number(
                        serde_json::Number::from_f64(elapsed)
                            .unwrap_or_else(|| serde_json::Number::from(0)),
                    ),
                );
                if let Ok(line) = event.to_json_line() {
                    let _ = state.log_tx.send(line);
                }
                return Ok(bad_gateway_response());
            }
        },
    };

    let traceparent_value = format!("00-{}-{}-01", trace_id, span_id);

    let mut upstream_req = Request::new(Body::from(req_body_bytes.clone()));
    *upstream_req.method_mut() = method;
    *upstream_req.uri_mut() = upstream_uri;
    {
        let headers = upstream_req.headers_mut();
        for (k, v) in &req_headers {
            if k.as_str().eq_ignore_ascii_case("host")
                || k.as_str().eq_ignore_ascii_case("traceparent")
            {
                continue;
            }
            headers.append(k.clone(), v.clone());
        }
        headers.insert(
            "traceparent",
            hyper::header::HeaderValue::from_str(&traceparent_value).unwrap_or_else(|_| {
                hyper::header::HeaderValue::from_static(
                    "00-00000000000000000000000000000000-0000000000000000-01",
                )
            }),
        );
    }

    let upstream_result = state.client.request(upstream_req).await;

    let elapsed = start.elapsed().as_secs_f64() * 1000.0;
    event.set_attr(
        "server.duration_ms",
        Value::Number(
            serde_json::Number::from_f64(elapsed).unwrap_or_else(|| serde_json::Number::from(0)),
        ),
    );

    let response = match upstream_result {
        Ok(resp) => {
            let status = resp.status().as_u16();
            let resp_headers = resp.headers().clone();
            let resp_body_bytes = hyper::body::to_bytes(resp.into_body())
                .await
                .unwrap_or_default();

            event.set_attr("http.response.status_code", Value::Number(status.into()));
            event.set_attr(
                "http.response.headers",
                redact_headers(&resp_headers, &state.config.redact_headers),
            );
            for (k, v) in body_attrs("http.response", &resp_body_bytes, state.config.body_max) {
                event.set_attr(&k, v);
            }

            let mut response = Response::new(Body::from(resp_body_bytes));
            *response.status_mut() = StatusCode::from_u16(status).unwrap_or(StatusCode::OK);
            for (k, v) in &resp_headers {
                response.headers_mut().append(k.clone(), v.clone());
            }
            response
        }
        Err(err) => {
            attach_proxy_error(&mut event, err.to_string(), state.config.body_max);
            bad_gateway_response()
        }
    };

    if let Ok(line) = event.to_json_line() {
        let _ = state.log_tx.send(line);
    }

    Ok(response)
}

pub async fn run(config: TapConfig) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let listen = config.listen;

    let writer: Box<dyn Write + Send> = match &config.out {
        OutputTarget::Stdout => Box::new(std::io::stdout()),
        OutputTarget::File(path) => {
            let file = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)?;
            Box::new(file)
        }
    };

    let (log_tx, log_rx) = mpsc::channel::<String>();
    std::thread::spawn(move || {
        let mut writer = BufWriter::new(writer);
        while let Ok(line) = log_rx.recv() {
            let _ = writeln!(writer, "{}", line);
            let _ = writer.flush();
        }
        let _ = writer.flush();
    });

    let state = Arc::new(ProxyState {
        config,
        client: Client::new(),
        log_tx,
    });

    let make_svc = make_service_fn(move |_conn| {
        let state = state.clone();
        async move { Ok::<_, hyper::Error>(service_fn(move |req| proxy_handler(req, state.clone()))) }
    });

    eprintln!("logtape tap http listening on {}", listen);
    Server::bind(&listen).serve(make_svc).await?;
    Ok(())
}
