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
use cli::{Args, SmokeCommand};
use domain::{
    uav_route_verify, uav_showcase_up, uav_showcase_verify, uav_sim_verify, uav_world_publish,
};
use support::InstalledTarget;

#[tokio::main]
async fn main() -> Result<()> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    match Args::parse().command {
        SmokeCommand::UavRouteVerify {
            conformance_bin,
            scenario,
            installation,
        } => {
            uav_route_verify(
                &conformance_bin,
                &scenario,
                &InstalledTarget::load(&installation)?,
            )
            .await
        }
        SmokeCommand::UavWorldPublish {
            conformance_bin,
            scenario,
            installation,
            output,
        } => {
            uav_world_publish(
                &conformance_bin,
                &scenario,
                &InstalledTarget::load(&installation)?,
                &output,
            )
            .await
        }
        SmokeCommand::UavDomainVerify {
            conformance_bin,
            scenario,
            installation,
        } => {
            uav_sim_verify(
                &conformance_bin,
                &scenario,
                &InstalledTarget::load(&installation)?,
            )
            .await
        }
        SmokeCommand::UavShowcaseUp {
            conformance_bin,
            scenario,
            installation,
        } => {
            uav_showcase_up(
                &conformance_bin,
                &scenario,
                &InstalledTarget::load(&installation)?,
            )
            .await
        }
        SmokeCommand::UavShowcaseVerify {
            conformance_bin,
            scenario,
            installation,
            chrome_cdp_url,
            evidence_root,
        } => {
            uav_showcase_verify(
                &conformance_bin,
                &scenario,
                &InstalledTarget::load(&installation)?,
                &chrome_cdp_url,
                &evidence_root,
            )
            .await
        }
    }
}
