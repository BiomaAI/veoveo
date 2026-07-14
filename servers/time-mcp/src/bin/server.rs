#[tokio::main]
async fn main() -> anyhow::Result<()> {
    veoveo_time_mcp::run().await
}
