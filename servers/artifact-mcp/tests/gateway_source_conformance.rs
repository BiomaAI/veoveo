//! Installed source conformance over one explicitly selected disposable Artifact.
use anyhow::Result;
use serde::Deserialize;
use veoveo_artifact_mcp::contract::{ArtifactId, ArtifactResource};
use veoveo_mcp_conformance::{
    KnowledgeRoute, KnowledgeSourceTarget, knowledge_probes::*, run_knowledge_source_conformance,
};
use veoveo_types::AccessSubject;

#[path = "support/read_grant_probe.rs"]
mod grant;
#[path = "../../../testing/installed/knowledge.rs"]
mod installed;
#[path = "../../../testing/installed/restart.rs"]
mod restart;

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
    let caller = input.installation.caller().await?;
    let metadata: veoveo_artifact_mcp::contract::ArtifactMetadata = installed::read(
        caller.peer(),
        &ArtifactResource::Metadata(input.artifact).to_uri(),
    )
    .await?;
    anyhow::ensure!(
        metadata.artifact_id() == input.artifact,
        "caller cannot read the selected Artifact fixture"
    );
    installed::close(caller).await?;
    let administrator = input.administrator.connect(&installation).await?;
    let driver = grant::GrantProbe::new(
        administrator.peer().clone(),
        input.artifact,
        input.grantee,
        restart::DeploymentRestart::new(
            &installation,
            &input.installation.deployment,
            "artifact-mcp",
        )?,
    )
    .await?;
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
            &driver,
        )],
        searches: vec![],
    };
    let result =
        run_knowledge_source_conformance(&target, &input.installation.credentials()?, &probes)
            .await;
    let cleanup = driver.cleanup().await;
    let closed = installed::close(administrator).await;
    cleanup?;
    closed?;
    input.installation.report(&result?)
}
