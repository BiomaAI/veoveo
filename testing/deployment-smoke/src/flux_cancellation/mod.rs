//! Exercise obsolete health-check cancellation without touching application releases.
mod fixture;
mod observe;

use std::{
    fs,
    path::PathBuf,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, ensure};
use serde::Serialize;
use veoveo_deploy_contract::RegistryTransport;

use fixture::Fixture;
use observe::Snapshot;

#[derive(Debug, clap::Args)]
pub(crate) struct Args {
    #[arg(long)]
    context: String,
    #[arg(long)]
    push_registry: String,
    #[arg(long)]
    pull_registry: String,
    #[arg(long, default_value = "tls", value_parser = parse_transport)]
    registry_transport: RegistryTransport,
    /// Digest-pinned image containing /bin/sh and sleep; only a readiness witness runs.
    #[arg(long)]
    image: String,
    #[arg(long)]
    evidence_output: PathBuf,
}

fn parse_transport(value: &str) -> Result<RegistryTransport, String> {
    match value {
        "tls" => Ok(RegistryTransport::Tls),
        "insecure-http" => Ok(RegistryTransport::InsecureHttp),
        _ => Err("expected tls or insecure-http".into()),
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Evidence {
    schema_version: &'static str,
    context: String,
    namespace: String,
    image: String,
    flux_cli: String,
    helm_cli: String,
    controller_timeout_seconds: u64,
    recovery_budget_seconds: u64,
    fix_elapsed_millis: Option<u128>,
    publications: Option<fixture::Publications>,
    observations: Vec<Snapshot>,
    failure: Option<String>,
    cleanup_failure: Option<String>,
}

pub(crate) fn verify(args: Args) -> Result<()> {
    ensure!(
        !args.evidence_output.exists(),
        "evidence output already exists"
    );
    if let Some(parent) = args.evidence_output.parent() {
        fs::create_dir_all(parent)?;
    }
    fixture::validate_inputs(&args)?;
    let tools = fixture::tool_versions()?;
    let started = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis();
    let namespace = format!("veoveo-cancel-{started}-{}", std::process::id());
    let mut evidence = Evidence {
        schema_version: "veoveo.io/flux-cancellation-evidence/v1",
        context: args.context.clone(),
        namespace: namespace.clone(),
        image: args.image.clone(),
        flux_cli: tools.0,
        helm_cli: tools.1,
        controller_timeout_seconds: 300,
        recovery_budget_seconds: 90,
        fix_elapsed_millis: None,
        publications: None,
        observations: vec![],
        failure: None,
        cleanup_failure: None,
    };
    let fixture = Fixture::new(&args, namespace)?;
    // Create refuses an existing namespace. Cleanup authority begins only on success.
    fixture.create_namespace()?;
    let outcome = exercise(&fixture, &mut evidence);
    let cleanup = fixture.cleanup();
    evidence.failure = outcome.as_ref().err().map(|error| format!("{error:#}"));
    evidence.cleanup_failure = cleanup.as_ref().err().map(|error| format!("{error:#}"));
    let file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&args.evidence_output)?;
    serde_json::to_writer_pretty(file, &evidence)?;
    println!(
        "Flux cancellation evidence: {}",
        args.evidence_output.display()
    );
    outcome?;
    cleanup?;
    Ok(())
}

fn exercise(fixture: &Fixture<'_>, evidence: &mut Evidence) -> Result<()> {
    let publications = fixture.publish()?;
    evidence.publications = Some(publications.clone());
    fixture.bootstrap(&publications)?;
    println!("Flux cancellation: establishing healthy release");
    evidence
        .observations
        .push(observe::wait(fixture, Duration::from_secs(180), |state| {
            state.ready(&publications.initial, "initial")
        })?);

    fixture.select(&publications.broken)?;
    println!("Flux cancellation: waiting for both controllers to check the broken revision");
    let blocked = observe::wait_stable(
        fixture,
        Duration::from_secs(90),
        Duration::from_secs(5),
        |state| state.checking_broken(&publications.broken),
    )?;
    let failed_generation = blocked.release.metadata.generation;
    evidence.observations.push(blocked);

    let fix_started = Instant::now();
    fixture.select(&publications.fixed)?;
    println!("Flux cancellation: submitted fixed source during active health checks");
    let recovered = observe::wait(fixture, Duration::from_secs(90), |state| {
        state.ready(&publications.fixed, "fixed")
            && state.release.metadata.generation > failed_generation
    });
    evidence.fix_elapsed_millis = Some(fix_started.elapsed().as_millis());
    // Preserve the last real resource state even when the latency assertion fails.
    evidence
        .observations
        .push(fixture.snapshot().context("reading final fixture state")?);
    recovered?;
    ensure!(
        fix_started.elapsed() < Duration::from_secs(90),
        "fix exceeded cancellation budget"
    );
    println!(
        "Flux cancellation: fixed release ready in {} ms",
        evidence.fix_elapsed_millis.unwrap_or_default()
    );
    Ok(())
}
