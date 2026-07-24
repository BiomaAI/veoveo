//! Hosted server implementation.
mod auth;
mod config;
mod host;
mod live_stream;
mod ownership;
mod prompts;
mod service;
mod state;
mod task_extension;
mod task_worker;

pub fn run() -> anyhow::Result<()> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    runtime.block_on(service::serve())
}
