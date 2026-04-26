use crate::tools::CalibreMcpServer;
use rmcp::ServiceExt;

pub async fn run_stdio_server(server: CalibreMcpServer) -> anyhow::Result<()> {
    tracing::info!("starting calibre-mcp stdio server");
    let running = server.serve(rmcp::transport::stdio()).await?;
    let _ = running.waiting().await?;
    Ok(())
}
