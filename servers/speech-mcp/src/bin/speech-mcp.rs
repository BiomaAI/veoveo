#[tokio::main]
async fn main() -> anyhow::Result<()> {
    veoveo_speech_mcp::server::run().await
}
