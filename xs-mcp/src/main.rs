use anyhow::Context;
use autolibre_mcp::{
    tools::CalibreMcpServer,
    transport::{sse::run_sse_server, stdio::run_stdio_server},
};
use backend::config::AppConfig;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TransportKind {
    Stdio,
    Sse,
}

#[derive(Debug, Clone, Copy)]
struct CliArgs {
    transport: TransportKind,
    port: u16,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_tracing();

    let args = parse_args(std::env::args().skip(1).collect())?;
    let config = backend::config::load_config().await?;
    validate_configured_llm_endpoints(&config)?;

    let db = backend::db::connect_sqlite_pool(&config.database.url, 5)
        .await
        .with_context(|| format!("connect sqlite pool {}", config.database.url))?;

    match args.transport {
        TransportKind::Stdio => {
            let server = CalibreMcpServer::new(db, config)?;
            run_stdio_server(server).await?;
        }
        TransportKind::Sse => {
            let server = CalibreMcpServer::new(db.clone(), config.clone())?;
            run_sse_server(db, config, server, args.port).await?;
        }
    }

    Ok(())
}

fn init_tracing() {
    let _ = tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .try_init();
}

fn parse_args(args: Vec<String>) -> anyhow::Result<CliArgs> {
    let mut transport = TransportKind::Stdio;
    let mut port: u16 = 8084;
    let mut index = 0usize;

    while index < args.len() {
        match args[index].as_str() {
            "--transport" => {
                index += 1;
                let value = args.get(index).context("missing value for --transport")?;
                transport = match value.as_str() {
                    "stdio" => TransportKind::Stdio,
                    "sse" => TransportKind::Sse,
                    _ => anyhow::bail!("unsupported transport: {value}"),
                };
            }
            "--port" => {
                index += 1;
                let value = args.get(index).context("missing value for --port")?;
                port = value
                    .parse::<u16>()
                    .with_context(|| format!("invalid port: {value}"))?;
            }
            "--help" | "-h" => {
                print_help();
                std::process::exit(0);
            }
            unknown => anyhow::bail!("unknown argument: {unknown}"),
        }
        index += 1;
    }

    Ok(CliArgs { transport, port })
}

fn print_help() {
    eprintln!(
        "Usage: autolibre-mcp [--transport stdio|sse] [--port <u16>]\n\
         Defaults: --transport stdio, --port 8084"
    );
}

fn validate_configured_llm_endpoints(config: &AppConfig) -> anyhow::Result<()> {
    for endpoint in [
        config.llm.librarian.endpoint.as_str(),
        config.llm.architect.endpoint.as_str(),
    ] {
        if endpoint.trim().is_empty() {
            continue;
        }
        backend::config::validate_llm_endpoint(endpoint, config.llm.allow_private_endpoints)
            .map_err(|err| anyhow::anyhow!("invalid llm endpoint {endpoint}: {err}"))?;
    }
    Ok(())
}
