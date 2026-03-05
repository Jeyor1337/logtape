# logtape

一个 Rust CLI 工具，作为本地 HTTP 反向代理运行，将请求/响应对录制为结构化 NDJSON 事件。同时提供 schema 校验和格式转换功能。

## 功能特性

- **`tap http`** — 透明反向代理，将每个 HTTP 请求/响应捕获为一条 NDJSON 事件行，包含方法、路径、请求头、body 元数据、耗时和追踪上下文。
- **`lint`** — 根据 v0.1 schema 校验 NDJSON 事件文件（字段类型、trace ID 格式、必需的 HTTP 属性）。支持 `--strict` 模式和 JSON 输出。
- **`fmt`** — 在 JSONL 和 logfmt 格式之间相互转换，支持无损往返。
- **W3C Traceparent** — 提取或生成 `trace_id` / `span_id`，向上游请求注入 `traceparent` 请求头。
- **数据脱敏** — 默认自动脱敏敏感请求头（`authorization`、`cookie`、`set-cookie`）和查询参数（`token`、`auth`），完全可配置。
- **Body 捕获** — 记录请求和响应 body 的大小、SHA-256 哈希、截断预览（UTF-8 或 base64）和编码方式。

## 安装

```bash
cargo install --path .
```

或从源码构建：

```bash
cargo build --release
```

二进制文件位于 `target/release/logtape`。

## 快速开始

### 1. 启动上游服务器

使用内置的 demo 服务器：

```bash
cargo run --example upstream_demo
```

这会在 `127.0.0.1:8080` 启动一个简单的 echo 服务器。

### 2. 启动录制代理

```bash
logtape tap http \
  --listen 127.0.0.1:9000 \
  --upstream http://127.0.0.1:8080 \
  --out tape.jsonl
```

### 3. 通过代理发送请求

```bash
curl -i "http://127.0.0.1:9000/hello?token=abc&x=1" \
  -H "authorization: Bearer secret" \
  -d "hello logtape"
```

### 4. 查看录制的事件

```bash
cat tape.jsonl
```

示例输出：

```json
{"v":"0.1","ts":"2026-03-04T12:00:00.123456789Z","kind":"http","name":"http.server","trace_id":"0123456789abcdef0123456789abcdef","span_id":"fedcba9876543210","attrs":{"http.method":"POST","url.path":"/hello","url.query":"token=[REDACTED]&x=1","http.request.headers":{"authorization":"[REDACTED]"},"http.response.status_code":200,"server.duration_ms":8.42}}
```

### 5. 转换为 logfmt 格式

```bash
logtape fmt --to logfmt tape.jsonl
```

输出：

```
kind=http name=http.server span_id=fedcba9876543210 trace_id=0123456789abcdef0123456789abcdef ts=2026-03-04T12:00:00.123456789Z v=0.1 a.http.method=POST a.url.path=/hello a.url.query="token=[REDACTED]&x=1" a.http.response.status_code=200 a.server.duration_ms=8.42
```

### 6. 将 logfmt 转回 JSONL

```bash
logtape fmt --to jsonl tape.logfmt
```

### 7. 校验事件

```bash
logtape lint tape.jsonl
```

使用 `--strict` 将警告视为错误：

```bash
logtape lint tape.jsonl --strict
```

以 JSON 格式输出诊断信息：

```bash
logtape lint tape.jsonl --json
```

## CLI 参考

### `logtape tap http`

启动录制反向代理。

| 参数 | 默认值 | 说明 |
|------|--------|------|
| `--listen` | （必需） | 本地监听地址（如 `127.0.0.1:9000`） |
| `--upstream` | （必需） | 上游服务器 URL |
| `--out` | `-`（标准输出） | 输出文件路径，`-` 表示标准输出 |
| `--service` | （无） | 包含在事件中的服务名称 |
| `--service-version` | （无） | 包含在事件中的服务版本 |
| `--body-max` | `4096` | Body 预览最大字节数 |
| `--redact-headers` | `authorization,cookie,set-cookie` | 需脱敏的请求头（CSV，不区分大小写） |
| `--redact-query` | `token,auth` | 需脱敏的查询参数（CSV） |

### `logtape lint`

根据 v0.1 schema 校验 NDJSON 事件文件。

```
logtape lint <input.jsonl> [--strict] [--json]
```

| 参数 | 说明 |
|------|------|
| `--strict` | 将警告视为错误（非零退出码） |
| `--json` | 以 JSON 格式输出诊断信息 |

### `logtape fmt`

在事件格式之间转换。

```
logtape fmt --to <format> <input>
```

| 格式 | 方向 | 说明 |
|------|------|------|
| `logfmt` | JSONL -> logfmt | 属性字段以 `a.` 前缀展平 |
| `jsonl` | logfmt -> JSONL | 恢复嵌套结构 |

## 事件 Schema (v0.1)

每个事件是一行 JSON，包含以下顶层字段：

| 字段 | 类型 | 说明 |
|------|------|------|
| `v` | string | Schema 版本（`"0.1"`） |
| `ts` | string | RFC 3339 时间戳（含纳秒） |
| `kind` | string | 事件类型（如 `"http"`） |
| `name` | string | 事件名称（如 `"http.server"`） |
| `trace_id` | string | 32 位小写十六进制 |
| `span_id` | string | 16 位小写十六进制 |
| `attrs` | object | 类型相关的属性 |
| `level` | string | （可选）代理失败时为 `"error"` |
| `service` | string | （可选）来自 `--service` |
| `service_version` | string | （可选）来自 `--service-version` |

完整规范请参阅 [spec/v0.1.md](spec/v0.1.md)。

## 测试

```bash
cargo test --tests
```

## 许可证

[MIT](LICENSE)
