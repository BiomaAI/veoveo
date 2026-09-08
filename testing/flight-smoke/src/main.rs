use anyhow::{Context, Result, bail, ensure};
use clap::Parser;
use serde_json::Value;
use std::{fs, time::Duration};

#[allow(dead_code)]
#[path = "../../browser-smoke/src/browser.rs"]
mod browser;
mod cli;
mod domain;
mod support;
#[allow(dead_code)]
#[path = "../../../servers/stream-mcp/src/contract/live.rs"]
mod wire;
use cli::{Args, SmokeCommand};
use domain::{uav_showcase_up, uav_showcase_verify, uav_sim_verify};

#[tokio::main]
async fn main() -> Result<()> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    match Args::parse().command {
        SmokeCommand::UavDomainVerify {
            conformance_bin,
            scenario,
            context,
            public_base_url,
        } => uav_sim_verify(&conformance_bin, &scenario, &context, &public_base_url).await,
        SmokeCommand::UavShowcaseUp {
            conformance_bin,
            scenario,
            context,
            namespace,
            public_base_url,
        } => {
            uav_showcase_up(
                &conformance_bin,
                &scenario,
                &context,
                &namespace,
                &public_base_url,
            )
            .await
        }
        SmokeCommand::UavShowcaseVerify {
            conformance_bin,
            scenario,
            context,
            namespace,
            public_base_url,
            chrome_cdp_url,
            evidence_root,
        } => {
            uav_showcase_verify(
                &conformance_bin,
                &scenario,
                &context,
                &namespace,
                &public_base_url,
                &chrome_cdp_url,
                &evidence_root,
            )
            .await
        }
    }
}
