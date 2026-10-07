//! Public-gateway qualification of Map's selected knowledge-source contract.
use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use std::{path::PathBuf, sync::Arc, time::Duration};
use veoveo_map_mcp::contract::*;
use veoveo_mcp_conformance::{knowledge_probes::*, *};

#[path = "support/source_authoring.rs"]
mod authoring;
use veoveo_testing_support::installed::knowledge as installed;
#[path = "support/source_publications.rs"]
mod publications;
#[path = "support/source_releases.rs"]
mod releases;
use veoveo_testing_support::installed::restart;
use veoveo_testing_support::installed::tools;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Input {
    installation: installed::InstalledSource,
    layer: FeatureLayerId,
    feature: MapFeatureId,
    publication_layers: [FeatureLayerId; 2],
    releases: releases::Releases,
    search: SearchLocationsRequest,
    expected: Vec<MapKnowledgeMember>,
    restricted_token_file: PathBuf,
}

#[tokio::test]
#[ignore = "requires deployed Map, populated collections, disposable authored/release fixtures, private caller tokens and Kubernetes access"]
async fn map_sources_conform_through_gateway_across_changes_creation_and_restart() -> Result<()> {
    run().await
}

async fn run() -> Result<()> {
    let input: Input = installed::input()?;
    let installation = input.installation.validate()?;
    ensure!(
        input.publication_layers[0] != input.publication_layers[1]
            && input.publication_layers.iter().all(|id| id != &input.layer),
        "Map publication fixtures need distinct layers"
    );
    ensure!(
        !input.expected.is_empty()
            && input.expected.iter().all(|m| matches!(
                m,
                MapKnowledgeMember::Location { .. } | MapKnowledgeMember::Facility { .. }
            )),
        "search fixture must name geography members"
    );
    let restricted_credentials = installed::credentials(&input.restricted_token_file)?;
    let credentials = input.installation.credentials()?;
    let caller = input.installation.caller().await?;
    let restart = Arc::new(restart::DeploymentRestart::new(
        &installation,
        &input.installation.deployment,
        "map-mcp",
        caller.peer().clone(),
        veoveo_mcp_contract::ServerResourceUris::new("map".parse()?).contract_uri(),
    )?);
    let layers = authoring::Authoring::new(
        caller.peer().clone(),
        authoring::Mutation::Layer(input.layer.clone()),
        restart.clone(),
    );
    let features = authoring::Authoring::new(
        caller.peer().clone(),
        authoring::Mutation::Feature {
            layer: input.layer.clone(),
            feature: input.feature.clone(),
        },
        restart.clone(),
    );
    let publications = publications::Publications::new(
        caller.peer().clone(),
        input.publication_layers.clone(),
        restart.clone(),
    );
    for id in [
        &input.layer,
        &input.publication_layers[0],
        &input.publication_layers[1],
    ] {
        ensure!(
            layers.layer(id).await?.archived_at.is_none(),
            "fixture layer is already archived"
        );
    }
    let releases =
        releases::ReleaseProbe::new(input.releases, caller.peer().clone(), restart).await?;
    let source = KnowledgeSourceTarget::new(
        input.installation.endpoint.as_str().parse()?,
        "map".parse()?,
        ["map".parse()?].into(),
        KnowledgeRoute::Gateway,
    )?;
    let probes = KnowledgeProbes {
        changes: vec![
            KnowledgeChangeProbe::update(
                MapKnowledgeCollection::Layers
                    .descriptor()
                    .collection()
                    .clone(),
                MapKnowledgeMember::Layer {
                    layer: input.layer.clone(),
                }
                .to_uri(),
                &layers,
            ),
            KnowledgeChangeProbe::update(
                MapKnowledgeCollection::Features
                    .descriptor()
                    .collection()
                    .clone(),
                MapKnowledgeMember::Feature {
                    layer: input.layer.clone(),
                    feature: input.feature.clone(),
                }
                .to_uri(),
                &features,
            ),
            KnowledgeChangeProbe::create(
                MapKnowledgeCollection::Publications
                    .descriptor()
                    .collection()
                    .clone(),
                &publications,
            ),
            KnowledgeChangeProbe::update(
                MapKnowledgeCollection::Releases
                    .descriptor()
                    .collection()
                    .clone(),
                MapKnowledgeMember::Release {
                    dataset: releases.selection.dataset.clone(),
                    release: releases.selection.candidate.clone(),
                }
                .to_uri(),
                &releases,
            ),
        ],
        searches: vec![KnowledgeSearchProbe {
            tool: "search_locations".parse()?,
            arguments: serde_json::to_value(&input.search)?
                .as_object()
                .cloned()
                .context("Map search input must be an object")?,
            expected: input
                .expected
                .iter()
                .map(MapKnowledgeMember::to_uri)
                .collect(),
            restricted_credentials,
            restricted: KnowledgeSearchAccess::Denied,
        }],
    };
    let result = run_knowledge_source_conformance(&source, &credentials, &probes).await;
    let reported = result.and_then(|report| input.installation.report(&report));
    // Attempt every owned cleanup, even if an earlier cleanup failed.
    let cleanup = tokio::time::timeout(Duration::from_secs(120), async {
        let release = tokio::time::timeout(Duration::from_secs(35), releases.cleanup())
            .await
            .context("release restoration exceeded 35 seconds")
            .and_then(|result| result);
        let mut authored = Ok(());
        for id in [
            &input.layer,
            &input.publication_layers[0],
            &input.publication_layers[1],
        ] {
            let result = tokio::time::timeout(Duration::from_secs(25), layers.archive(id))
                .await
                .context("layer archival exceeded 25 seconds")
                .and_then(|result| result);
            if result.is_err() {
                authored = result;
            }
        }
        release?;
        authored
    })
    .await
    .context("Map fixture cleanup exceeded two minutes");
    let closed = installed::close(caller).await;
    cleanup??;
    closed?;
    reported
}
