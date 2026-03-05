# logtape

A Rust CLI tool that acts as a local HTTP reverse proxy, recording request/response pairs as structured NDJSON events. It also provides schema linting and format conversion utilities.

## Features

- **`tap http`** — Transparent reverse proxy that captures every HTTP request/response into a single NDJSON event line, including method, path, headers, body metadata, timing, and trace context.
- **`lint`** — Validates NDJSON event files against the v0.1 schema (field types, trace ID format, required HTTP attrs). Supports `--strict` mode and JSON output.
- **`fmt`** — Converts between JSONL and logfmt formats with roundtrip fidelity.
- **W3C Traceparent** — Extracts or generates `trace_id` / `span_id`, injects `traceparent` header into upstream requests.
- **Redaction** — Automatically redacts sensitive headers (`authorization`, `cookie`, `set-cookie`) and query parameters (`token`, `auth`) by default; fully configurable.
- **Body capture** — Records body size, SHA-256 hash, truncated preview (UTF-8 or base64), and encoding for both request and response.

## Installation

```bash
cargo install --path .
```

Or build from source:

```bash
cargo build --release
```

The binary will be at `target/release/logtape`.

## Quick Start

### 1. Start an upstream server

Use the built-in demo server:

```bash
cargo run --example upstream_demo
```

This starts a simple echo server on `127.0.0.1:8080`.

### 2. Start the recording proxy

```bash
logtape tap http \
  --listen 127.0.0.1:9000 \
  --upstream http://127.0.0.1:8080 \
  --out tape.jsonl
```

### 3. Send a request through the proxy

```bash
curl -i "http://127.0.0.1:9000/hello?token=abc&x=1" \
  -H "authorization: Bearer secret" \
  -d "hello logtape"
```

### 4. View the recorded event

```bash
cat tape.jsonl
```

Sample output:

```json
{"v":"0.1","ts":"2026-03-04T12:00:00.123456789Z","kind":"http","name":"http.server","trace_id":"0123456789abcdef0123456789abcdef","span_id":"fedcba9876543210","attrs":{"http.method":"POST","url.path":"/hello","url.query":"token=[REDACTED]&x=1","http.request.headers":{"authorization":"[REDACTED]"},"http.response.status_code":200,"server.duration_ms":8.42}}
```

### 5. Convert to logfmt

```bash
logtape fmt --to logfmt tape.jsonl
```

Output:

```
kind=http name=http.server span_id=fedcba9876543210 trace_id=0123456789abcdef0123456789abcdef ts=2026-03-04T12:00:00.123456789Z v=0.1 a.http.method=POST a.url.path=/hello a.url.query="token=[REDACTED]&x=1" a.http.response.status_code=200 a.server.duration_ms=8.42
```

### 6. Convert logfmt back to JSONL

```bash
logtape fmt --to jsonl tape.logfmt
```

### 7. Validate events

```bash
logtape lint tape.jsonl
```

Use `--strict` to treat warnings as errors:

```bash
logtape lint tape.jsonl --strict
```

Output diagnostics as JSON:

```bash
logtape lint tape.jsonl --json
```

## CLI Reference

### `logtape tap http`

Start a recording reverse proxy.

| Flag | Default | Description |
|------|---------|-------------|
| `--listen` | (required) | Local listen address (e.g. `127.0.0.1:9000`) |
| `--upstream` | (required) | Upstream server URL |
| `--out` | `-` (stdout) | Output file path, or `-` for stdout |
| `--service` | (none) | Service name to include in events |
| `--service-version` | (none) | Service version to include in events |
| `--body-max` | `4096` | Max body preview bytes |
| `--redact-headers` | `authorization,cookie,set-cookie` | Headers to redact (CSV, case-insensitive) |
| `--redact-query` | `token,auth` | Query parameters to redact (CSV) |

### `logtape lint`

Validate NDJSON event files against the v0.1 schema.

```
logtape lint <input.jsonl> [--strict] [--json]
```

| Flag | Description |
|------|-------------|
| `--strict` | Treat warnings as errors (non-zero exit) |
| `--json` | Output diagnostics as JSON |

### `logtape fmt`

Convert between event formats.

```
logtape fmt --to <format> <input>
```

| Format | Direction | Description |
|--------|-----------|-------------|
| `logfmt` | JSONL -> logfmt | Flatten attrs with `a.` prefix |
| `jsonl` | logfmt -> JSONL | Restore nested structure |

## Event Schema (v0.1)

Each event is a single JSON line with these top-level fields:

| Field | Type | Description |
|-------|------|-------------|
| `v` | string | Schema version (`"0.1"`) |
| `ts` | string | RFC 3339 timestamp with nanoseconds |
| `kind` | string | Event kind (e.g. `"http"`) |
| `name` | string | Event name (e.g. `"http.server"`) |
| `trace_id` | string | 32-char lowercase hex |
| `span_id` | string | 16-char lowercase hex |
| `attrs` | object | Kind-specific attributes |
| `level` | string | (optional) `"error"` on proxy failure |
| `service` | string | (optional) From `--service` |
| `service_version` | string | (optional) From `--service-version` |

See [spec/v0.1.md](spec/v0.1.md) for the full specification.

## Testing

```bash
cargo test --tests
```

## License

[MIT](LICENSE)
