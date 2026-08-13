//! Hosted server implementation.
mod admin;
mod auth;
mod config;
mod host;
mod live_view;
mod live_view_audit;
mod ownership;
mod prompts;
mod runtime_events;
mod service;
mod signaling;
mod state;
mod task_extension;
mod task_worker;
mod world_bootstrap;

pub fn run() -> anyhow::Result<()> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    runtime.block_on(service::serve())
}
