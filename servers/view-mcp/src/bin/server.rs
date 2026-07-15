use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    veoveo_view_mcp::run().await
}
