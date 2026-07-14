use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    veoveo_map_mcp::run().await
}
