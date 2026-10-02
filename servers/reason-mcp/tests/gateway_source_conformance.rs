//! Source conformance reuses a completed analysis; no additional GPU inference.
use anyhow::Result;
use serde::Deserialize;
use veoveo_mcp_conformance::{
    KnowledgeRoute, KnowledgeSourceTarget, knowledge_probes::*, run_knowledge_source_conformance,
};
use veoveo_reason_mcp::contract::{AnalysisId, FindingCollection, FindingResource, FindingSummary};
use veoveo_types::{AccessSubject, ResourceAddress};

#[path = "../../artifact-mcp/tests/support/read_grant_probe.rs"]
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
    analysis: AnalysisId,
    grantee: AccessSubject,
}

#[tokio::test]
#[ignore = "requires installed Reason and Artifact, a completed analysis, private caller tokens and an absent result grantee"]
async fn findings_conform_through_gateway_across_grants_and_restart() -> Result<()> {
    run().await
}

async fn run() -> Result<()> {
    let input: Input = installed::input()?;
    let installation = input.installation.validate()?;
    let caller = input.installation.caller().await?;
    let resource = FindingResource::Member {
        collection: FindingCollection::Results,
        analysis: input.analysis,
    }
    .to_uri()?;
    let summary: FindingSummary = installed::read(caller.peer(), &resource).await?;
    anyhow::ensure!(
        summary.analysis_id() == input.analysis,
        "finding belongs to another analysis"
    );
    let result_artifact = summary.result_artifact().artifact_id();
    installed::close(caller).await?;
    let administrator = input.administrator.connect(&installation).await?;
    let driver = grant::GrantProbe::new(
        administrator.peer().clone(),
        result_artifact,
        input.grantee,
        restart::DeploymentRestart::new(
            &installation,
            &input.installation.deployment,
            "reason-mcp",
        )?,
    )
    .await?;
    let target = KnowledgeSourceTarget::new(
        input.installation.endpoint.as_str().parse()?,
        "reason".parse()?,
        ["reason".parse()?].into(),
        KnowledgeRoute::Gateway,
    )?;
    let probes = KnowledgeProbes {
        changes: FindingCollection::ALL
            .into_iter()
            .map(|collection| {
                Ok(KnowledgeChangeProbe::update(
                    collection.descriptor().collection().clone(),
                    FindingResource::Member {
                        collection,
                        analysis: input.analysis,
                    }
                    .to_uri()?,
                    &driver,
                ))
            })
            .collect::<Result<_>>()?,
        searches: vec![],
    };
    let result =
        run_knowledge_source_conformance(&target, &input.installation.credentials()?, &probes)
            .await;
    let reported = result.and_then(|report| input.installation.report(&report));
    let cleanup = driver.cleanup().await;
    let closed = installed::close(administrator).await;
    cleanup?;
    closed?;
    reported
}
