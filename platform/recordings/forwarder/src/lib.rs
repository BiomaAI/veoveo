//! Producer-side durable bridge from loopback Rerun gRPC to Veoveo recording ingest.

pub mod batch;
pub mod client;
pub mod config;
pub mod oauth;
pub mod queue;
pub mod runner;

pub use config::ForwarderConfig;
pub use runner::run;
