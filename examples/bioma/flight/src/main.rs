use anyhow::{Context, Result, bail, ensure};
use clap::Parser;
use serde_json::Value;
use std::{fs, time::Duration};

#[allow(dead_code)]
#[path = "../../acceptance/src/browser/browser.rs"]
mod browser;
mod cli;
mod domain;
#[path = "../../acceptance/src/browser/source_timeline.rs"]
mod source_timeline;
mod support;
use cli::{Args, SmokeCommand};
use domain::{
    uav_recording_verify, uav_route_verify, uav_showcase_up, uav_showcase_verify, uav_sim_verify,
    uav_stream_verify, uav_world_publish,
};
use support::InstalledTarget;

#[tokio::main]
async fn main() -> Result<()> {
    veoveo_testing_support::lifecycle::owner::run(execute()).await
}
async fn execute() -> Result<()> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    match Args::parse().command {
        SmokeCommand::UavStreamVerify {
            conformance_bin,
            scenario,
            installation,
        } => {
            let conformance_bin = veoveo_testing_support::artifacts::requested_executable(
                conformance_bin,
                "veoveo-mcp-conformance",
                "conformance",
            )?;

            uav_stream_verify(
                &conformance_bin,
                &scenario,
                &InstalledTarget::load(&installation)?,
            )
            .await
        }
        SmokeCommand::UavRecordingVerify {
            conformance_bin,
            scenario,
            installation,
        } => {
            let conformance_bin = veoveo_testing_support::artifacts::requested_executable(
                conformance_bin,
                "veoveo-mcp-conformance",
                "conformance",
            )?;

            uav_recording_verify(
                &conformance_bin,
                &scenario,
                &InstalledTarget::load(&installation)?,
            )
            .await
        }
        SmokeCommand::UavRouteVerify {
            conformance_bin,
            scenario,
            installation,
        } => {
            let conformance_bin = veoveo_testing_support::artifacts::requested_executable(
                conformance_bin,
                "veoveo-mcp-conformance",
                "conformance",
            )?;

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
            let conformance_bin = veoveo_testing_support::artifacts::requested_executable(
                conformance_bin,
                "veoveo-mcp-conformance",
                "conformance",
            )?;

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
            let conformance_bin = veoveo_testing_support::artifacts::requested_executable(
                conformance_bin,
                "veoveo-mcp-conformance",
                "conformance",
            )?;

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
            let conformance_bin = veoveo_testing_support::artifacts::requested_executable(
                conformance_bin,
                "veoveo-mcp-conformance",
                "conformance",
            )?;

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
            let conformance_bin = veoveo_testing_support::artifacts::requested_executable(
                conformance_bin,
                "veoveo-mcp-conformance",
                "conformance",
            )?;

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
