//! Hosted server implementation.
mod admin;
mod auth;
mod config;
mod host;
mod ownership;
mod prompts;
mod service;
mod state;
mod task_extension;
mod task_worker;

#[cfg(test)]
pub(crate) use service::fake_state;

pub fn run() -> anyhow::Result<()> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    runtime.block_on(service::serve())
}
