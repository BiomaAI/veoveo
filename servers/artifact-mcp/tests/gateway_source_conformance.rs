//! Installed source conformance over one explicitly selected disposable Artifact.
use anyhow::{Context, Result};
use serde::Deserialize;
use veoveo_artifact_mcp::contract::{ArtifactId, ArtifactResource};
use veoveo_mcp_conformance::{
    KnowledgeRoute, KnowledgeSourceTarget, knowledge_probes::*, run_knowledge_source_conformance,
};
use veoveo_types::AccessSubject;

#[path = "support/read_grant_probe.rs"]
mod grant;
use veoveo_testing_support::installed::knowledge as installed;
use veoveo_testing_support::installed::restart;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Input {
    installation: installed::InstalledSource,
    administrator: grant::Administrator,
    artifact: ArtifactId,
    grantee: AccessSubject,
}

#[tokio::test]
#[ignore = "requires installed Artifact, private operator/admin tokens and a disposable Artifact with an absent grantee"]
async fn metadata_conforms_through_gateway_across_grants_and_restart() -> Result<()> {
    run().await
}

async fn run() -> Result<()> {
    let input: Input = installed::input()?;
    let installation = input.installation.validate()?;
    let end = tokio::time::Instant::now() + std::time::Duration::from_secs(900);
    let mut caller_slot = None;
    let mut administrator_slot = None;
    let mut driver_slot = None;
    let mut mutation_admitted = false;
    let reported = tokio::time::timeout_at(end, async {
        caller_slot = Some(input.installation.caller().await?);
        let caller = caller_slot.as_ref().context("source caller absent")?;
        let metadata: veoveo_artifact_mcp::contract::ArtifactMetadata = installed::read(
            caller.peer(),
            &ArtifactResource::Metadata(input.artifact).to_uri(),
        )
        .await?;
        anyhow::ensure!(
            metadata.artifact_id() == input.artifact,
            "caller cannot read the selected Artifact fixture"
        );
        administrator_slot = Some(input.administrator.connect(&installation).await?);
        let administrator = administrator_slot
            .as_ref()
            .context("source administrator absent")?;
        driver_slot = Some(
            grant::GrantProbe::new(
                administrator.peer().clone(),
                input.artifact,
                input.grantee,
                restart::DeploymentRestart::new(
                    &installation,
                    &input.installation.deployment,
                    "artifact-mcp",
                    administrator.peer().clone(),
                    veoveo_mcp_contract::ServerResourceUris::new("artifact".parse()?)
                        .contract_uri(),
                )?,
            )
            .await?,
        );
        let driver = driver_slot.as_ref().context("source driver absent")?;
        let target = KnowledgeSourceTarget::new(
            input.installation.endpoint.as_str().parse()?,
            "artifact".parse()?,
            ["artifact".parse()?].into(),
            KnowledgeRoute::Gateway,
        )?;
        let probes = KnowledgeProbes {
            changes: vec![KnowledgeChangeProbe::update(
                veoveo_artifact_mcp::knowledge::collection()
                    .collection()
                    .clone(),
                ArtifactResource::Metadata(input.artifact).to_uri(),
                driver,
            )],
            searches: vec![],
        };
        mutation_admitted = true;
        let result =
            run_knowledge_source_conformance(&target, &input.installation.credentials()?, &probes)
                .await;
        result.and_then(|report| input.installation.report(&report))
    })
    .await
    .context("source operation exceeded fifteen minutes")
    .and_then(|result| result);
    // Retain clients and the admitted mutation owner across work cancellation.
    let cleanup_end = tokio::time::Instant::now() + std::time::Duration::from_secs(40);
    let cleanup = if let Some(driver) = driver_slot.as_ref().filter(|_| mutation_admitted) {
        tokio::time::timeout_at(
            cleanup_end - std::time::Duration::from_secs(10),
            driver.cleanup(),
        )
        .await
        .context("source fixture cleanup deadline elapsed")
        .and_then(|result| result)
    } else {
        Ok(())
    };
    let mut closed = Ok(());
    for connection in [caller_slot.take(), administrator_slot.take()]
        .into_iter()
        .flatten()
    {
        let result = tokio::time::timeout_at(cleanup_end, installed::close(connection))
            .await
            .context("source connection cleanup deadline elapsed")
            .and_then(|result| result);
        if result.is_err() {
            closed = result;
        }
    }
    cleanup?;
    closed?;
    reported
}
