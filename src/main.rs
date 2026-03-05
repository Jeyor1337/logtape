use clap::Parser;
use logtape::cli::{parse_csv_set, Cli, Commands, TapProtocol};
use logtape::tap::http::{OutputTarget, TapConfig};
use std::io::{BufReader, Write};

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Tap { protocol } => match protocol {
            TapProtocol::Http {
                listen,
                upstream,
                out,
                service,
                service_version,
                body_max,
                redact_headers,
                redact_query,
            } => {
                let config = TapConfig {
                    listen,
                    upstream,
                    out: if out == "-" {
                        OutputTarget::Stdout
                    } else {
                        OutputTarget::File(out)
                    },
                    service,
                    service_version,
                    body_max,
                    redact_headers: parse_csv_set(&redact_headers),
                    redact_query: parse_csv_set(&redact_query),
                };
                let rt = tokio::runtime::Runtime::new().unwrap_or_else(|e| {
                    eprintln!("error creating runtime: {}", e);
                    std::process::exit(1);
                });
                if let Err(e) = rt.block_on(logtape::tap::http::run(config)) {
                    eprintln!("error: {}", e);
                    std::process::exit(1);
                }
            }
        },
        Commands::Lint {
            input,
            strict,
            json,
        } => {
            let file = std::fs::File::open(&input).unwrap_or_else(|e| {
                eprintln!("error opening {}: {}", input, e);
                std::process::exit(1);
            });
            let reader = BufReader::new(file);
            let result = logtape::lint::lint::lint_reader(reader);

            if json {
                let diags: Vec<serde_json::Value> = result
                    .diagnostics
                    .iter()
                    .map(|d| {
                        serde_json::json!({
                            "line": d.line,
                            "level": d.level.to_string(),
                            "message": d.message,
                        })
                    })
                    .collect();
                println!(
                    "{}",
                    serde_json::to_string_pretty(&diags).unwrap_or_else(|_| "[]".to_string())
                );
            } else {
                for d in &result.diagnostics {
                    eprintln!("{}:{}: [{}] {}", input, d.line, d.level, d.message);
                }
                eprintln!(
                    "{} lines checked, {} diagnostics",
                    result.lines_checked,
                    result.diagnostics.len()
                );
            }

            if logtape::lint::lint::has_errors(&result.diagnostics, strict) {
                std::process::exit(1);
            }
        }
        Commands::Fmt { to, input } => match to.as_str() {
            "logfmt" => {
                let file = std::fs::File::open(&input).unwrap_or_else(|e| {
                    eprintln!("error opening {}: {}", input, e);
                    std::process::exit(1);
                });
                let reader = BufReader::new(file);
                let lines = logtape::fmt::fmt::jsonl_to_logfmt(reader);
                let stdout = std::io::stdout();
                let mut out = stdout.lock();
                for line in lines {
                    let _ = writeln!(out, "{}", line);
                }
            }
            "jsonl" => {
                let content = std::fs::read_to_string(&input).unwrap_or_else(|e| {
                    eprintln!("error opening {}: {}", input, e);
                    std::process::exit(1);
                });
                let lines = logtape::fmt::fmt::logfmt_to_jsonl(&content);
                let stdout = std::io::stdout();
                let mut out = stdout.lock();
                for line in lines {
                    let _ = writeln!(out, "{}", line);
                }
            }
            other => {
                eprintln!("unknown format: {}", other);
                std::process::exit(1);
            }
        },
    }
}
