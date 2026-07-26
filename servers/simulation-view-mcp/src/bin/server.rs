use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    veoveo_simulation_view_mcp::run().await
}
