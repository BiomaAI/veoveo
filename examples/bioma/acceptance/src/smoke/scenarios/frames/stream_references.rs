//! Synthetic stream-reference storage and current consumer refusal on installed data.
use super::*;
use anyhow::ensure;
use rmcp::{
    ServiceError,
    model::{CallToolRequestParams, ErrorCode},
};
use serde::Serialize;
use std::io::{Seek, SeekFrom, Write};
use veoveo_frames_mcp::contract::*;
use veoveo_mcp_conformance::client::failure::ObservedFailure;
use veoveo_testing_support::{SmokeMcpClient, connect_mcp_client};
use veoveo_types::{ResourceAddress, ResourceUriBuilder, ResourceUriParts, UriSegment};
use veoveo_uav_sim_mcp::contract::{SimulationWorldBinding, WorldBindingError};

const MAX_RECEIPT_BYTES: usize = 1024 * 1024;
#[derive(Debug, Clone, Copy, PartialEq, Eq, veoveo_types::Vocabulary)]
enum Outcome {
    #[vocabulary(rename = "pending")]
    Pending,
    #[vocabulary(rename = "unresolved")]
    Unresolved,
    #[vocabulary(rename = "failed_settled")]
    FailedSettled,
    #[vocabulary(rename = "passed")]
    Passed,
}
#[derive(Serialize)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
enum ConversionObservation {
    Mcp {
        failure: ObservedFailure,
    },
    OtherServiceFailure {
        observed: coverage::ForeignObservation,
    },
    UnexpectedSuccess,
    Deadline,
}
#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct Receipt {
    schema: &'static str,
    synthetic_reference: bool,
    retained_append_only: bool,
    world_id: Option<FrameWorldId>,
    create_intent: Option<CreateWorldRequest>,
    created: Option<CreateWorldOutput>,
    publish_intent: Option<PublishWorldRequest>,
    published: Option<PublishWorldOutput>,
    world_read: Option<FrameWorldSummary>,
    revision_read: Option<FrameWorldRevision>,
    dynamic_node_read: Option<FrameNode>,
    descendant_read: Option<FrameNode>,
    conversion_intent: Option<ConvertFrameRequest>,
    conversion: Option<ConversionObservation>,
    consumer_refused_dynamic: Option<bool>,
    requests: Vec<RequestObservation>,
    outcome: Option<Outcome>,
    client_closed: bool,
}
fn persist(file: &mut File, receipt: &Receipt) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(receipt)?;
    ensure!(
        bytes.len() <= MAX_RECEIPT_BYTES,
        "Frames reference receipt exceeds 1 MiB"
    );
    file.seek(SeekFrom::Start(0))?;
    file.write_all(&bytes)?;
    file.set_len(bytes.len().try_into()?)?;
    file.flush()?;
    file.sync_all()?;
    Ok(())
}
#[derive(Serialize)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
enum RequestIntent {
    Create { request: CreateWorldRequest },
    Publish { request: PublishWorldRequest },
    Read { uri: veoveo_types::ResourceUri },
    Convert { request: ConvertFrameRequest },
}
#[derive(Serialize)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
enum ResponseObservation {
    Awaiting,
    Response {
        digest: veoveo_types::Sha256Digest,
    },
    Failure {
        observed: coverage::ForeignObservation,
    },
    Deadline,
    Incomplete,
    ConversionRecorded,
}
#[derive(Serialize)]
struct RequestObservation {
    intent: RequestIntent,
    response: ResponseObservation,
}
fn admit_request_with(
    receipt: &mut Receipt,
    intent: RequestIntent,
    mut save: impl FnMut(&Receipt) -> Result<()>,
) -> Result<usize> {
    ensure!(
        receipt.requests.len() < 7,
        "Frames reference request budget exceeded"
    );
    let index = receipt.requests.len();
    receipt.requests.push(RequestObservation {
        intent,
        response: ResponseObservation::Awaiting,
    });
    if let Err(error) = save(receipt) {
        if let Some(request) = receipt.requests.pop() {
            match request.intent {
                RequestIntent::Create { .. } => receipt.create_intent = None,
                RequestIntent::Publish { .. } => receipt.publish_intent = None,
                RequestIntent::Convert { .. } => receipt.conversion_intent = None,
                RequestIntent::Read { .. } => {}
            }
        }
        return Err(error);
    }
    Ok(index)
}
async fn dispatch_after_intent<R, F: std::future::Future<Output = R>>(
    receipt: &mut Receipt,
    intent: RequestIntent,
    save: impl FnMut(&Receipt) -> Result<()>,
    request: impl FnOnce() -> F,
) -> Result<(usize, R)> {
    let index = admit_request_with(receipt, intent, save)?;
    Ok((index, request().await))
}
fn digest_response(response: &impl Serialize) -> Result<veoveo_types::Sha256Digest> {
    use sha2::{Digest, Sha256};
    let bytes = serde_json::to_vec(response)?;
    ensure!(
        bytes.len() <= MAX_RECEIPT_BYTES,
        "Frames reference response exceeds 1 MiB"
    );
    Ok(veoveo_types::Sha256Digest::from_bytes(
        Sha256::digest(bytes).into(),
    ))
}
async fn tool<T: serde::de::DeserializeOwned>(
    client: &SmokeMcpClient,
    file: &mut File,
    receipt: &mut Receipt,
    intent: RequestIntent,
    name: &str,
    request: &impl Serialize,
) -> Result<T> {
    let name = veoveo_gateway_contract::GatewayToolName::from_parts(
        &veoveo_types::ServerSlug::parse("frames")?,
        &veoveo_types::LocalToolName::parse(name)?,
    )?;
    let params = CallToolRequestParams::new(name.to_string())
        .with_arguments(serde_json::from_value(serde_json::to_value(request)?)?);
    let (index, observed) = dispatch_after_intent(
        receipt,
        intent,
        |receipt| persist(file, receipt),
        || tokio::time::timeout(Duration::from_secs(15), client.call_tool_once(params)),
    )
    .await?;
    let response = match observed {
        Ok(Ok(rmcp::model::CallToolResponse::Complete(response))) => response,
        Ok(Ok(_)) => {
            receipt.requests[index].response = ResponseObservation::Incomplete;
            persist(file, receipt)?;
            bail!("Frames reference tool did not return a complete response");
        }
        Ok(Err(error)) => {
            receipt.requests[index].response = ResponseObservation::Failure {
                observed: coverage::observe_peer_failure(error),
            };
            persist(file, receipt)?;
            bail!("Frames reference tool request failed");
        }
        Err(_) => {
            receipt.requests[index].response = ResponseObservation::Deadline;
            persist(file, receipt)?;
            bail!("Frames reference tool request exceeded fifteen seconds");
        }
    };
    receipt.requests[index].response = ResponseObservation::Response {
        digest: digest_response(&response)?,
    };
    persist(file, receipt)?;
    ensure!(
        response.is_error != Some(true),
        "Frames reference tool returned an error result"
    );
    serde_json::from_value(
        response
            .structured_content
            .context("Frames reference tool omitted structured content")?,
    )
    .map_err(|_| anyhow!("Frames reference tool failed owner decoding"))
}
async fn read<T: serde::de::DeserializeOwned>(
    client: &SmokeMcpClient,
    file: &mut File,
    receipt: &mut Receipt,
    uri: veoveo_types::ResourceUri,
) -> Result<T> {
    let (index, observed) = dispatch_after_intent(
        receipt,
        RequestIntent::Read { uri: uri.clone() },
        |receipt| persist(file, receipt),
        || {
            tokio::time::timeout(
                Duration::from_secs(15),
                client.read_resource(rmcp::model::ReadResourceRequestParams::new(uri.as_str())),
            )
        },
    )
    .await?;
    let response = match observed {
        Ok(Ok(response)) => response,
        Ok(Err(error)) => {
            receipt.requests[index].response = ResponseObservation::Failure {
                observed: coverage::observe_peer_failure(error),
            };
            persist(file, receipt)?;
            bail!("Frames reference read failed");
        }
        Err(_) => {
            receipt.requests[index].response = ResponseObservation::Deadline;
            persist(file, receipt)?;
            bail!("Frames reference read exceeded fifteen seconds");
        }
    };
    receipt.requests[index].response = ResponseObservation::Response {
        digest: digest_response(&response)?,
    };
    persist(file, receipt)?;
    let [
        rmcp::model::ResourceContents::TextResourceContents {
            uri: returned,
            text,
            ..
        },
    ] = response.contents.as_slice()
    else {
        bail!("Frames reference read requires one JSON resource");
    };
    ensure!(
        returned == uri.as_str(),
        "Frames reference read changed its URI"
    );
    serde_json::from_str(text).map_err(|_| anyhow!("Frames reference read failed owner decoding"))
}
fn synthetic_tree(world: &FrameWorldId) -> Result<FrameWorldTree> {
    let reference = FrameStreamUri::try_from(
        ResourceUriBuilder::from_parts(ResourceUriParts::parse("frames-fixture://synthetic")?)
            .segment(UriSegment::new(world.to_string())?)
            .build()?,
    )?;
    let mut tree = installed_frames_tree()?;
    tree.frames[2].frame_id = FrameId::parse("dynamic-node")?;
    tree.frames[2].parent_transform = Some(FrameParentTransform::DynamicStream {
        stream_uri: reference,
        entity_path: FrameEntityPath::new("/fixture/body")?,
    });
    tree.frames.push(FrameNode {
        frame_id: FrameId::parse("static-descendant")?,
        basis: FrameBasis::Enu,
        parent_frame_id: Some(FrameId::parse("dynamic-node")?),
        parent_transform: Some(FrameParentTransform::StaticRigid {
            translation_m: [1.0, 0.0, 0.0],
            rotation_xyzw: [0.0, 0.0, 0.0, 1.0],
        }),
        description: None,
    });
    Ok(ValidatedWorldTree::new(tree)?.tree().clone())
}
fn observed_conversion(error: ServiceError) -> ConversionObservation {
    match error {
        ServiceError::McpError(error) => ConversionObservation::Mcp {
            failure: ObservedFailure::mcp(i64::from(error.code.0), error.message.as_ref()),
        },
        other => ConversionObservation::OtherServiceFailure {
            observed: coverage::observe_peer_failure(other),
        },
    }
}
fn require_conversion_refusal(observed: &ConversionObservation) -> Result<()> {
    let expected = ObservedFailure::mcp(
        i64::from(ErrorCode::INVALID_PARAMS.0),
        "frame `dynamic-node` uses a dynamic transform stream and requires a timestamped recording query",
    );
    ensure!(
        matches!(observed, ConversionObservation::Mcp { failure } if failure == &expected),
        "Frames conversion did not return the owning dynamic-ancestry refusal"
    );
    Ok(())
}
fn require_readback(receipt: &Receipt) -> Result<()> {
    let request = receipt
        .publish_intent
        .as_ref()
        .context("missing publication intent")?;
    let published = receipt.published.as_ref().context("missing publication")?;
    let created = &receipt
        .created
        .as_ref()
        .context("missing admitted creation")?
        .world;
    let revision = receipt
        .revision_read
        .as_ref()
        .context("missing installed revision")?;
    ensure!(
        published.created
            && published.world.world_id() == created.world_id()
            && published.world.display_name == created.display_name
            && published.world.description == created.description
            && published.world.created_at == created.created_at
            && published.world.updated_at == published.revision.created_at()
            && published.world.revision() == 1
            && published.revision.revision().get() == 1
            && published.world.world_id() == request.world_id
            && published.revision.world_id() == request.world_id
            && published.world.head_revision_id() == Some(&published.revision.revision_id())
            && receipt.world_read.as_ref() == Some(&published.world)
            && revision == &published.revision
            && revision.tree() == &request.tree,
        "Frames installed publication or immutable read differs from admitted intent"
    );
    let validated = ValidatedWorldTree::new(request.tree.clone())?;
    ensure!(
        revision.spec_digest() == validated.spec_digest(),
        "Frames reference digest differs"
    );
    for (id, observed) in [
        ("dynamic-node", &receipt.dynamic_node_read),
        ("static-descendant", &receipt.descendant_read),
    ] {
        let expected = request
            .tree
            .frames
            .iter()
            .find(|node| node.frame_id.as_str() == id)
            .context("missing fixture node")?;
        ensure!(
            observed.as_ref() == Some(expected),
            "Frames node read changed reference, entity selector or parent"
        );
    }
    Ok(())
}
fn unresolved(receipt: &Receipt) -> bool {
    (receipt.create_intent.is_some() && receipt.created.is_none())
        || (receipt.publish_intent.is_some() && receipt.published.is_none())
        || (receipt.conversion_intent.is_some()
            && !matches!(
                receipt.conversion.as_ref(),
                Some(ConversionObservation::Mcp { .. })
            ))
}

pub(crate) async fn run(installation: &support::InstalledTarget, evidence: &Path) -> Result<()> {
    let mut file = admit_frames_evidence(evidence)?;
    let world_id = FrameWorldId::parse(format!("reference-{}", uuid::Uuid::new_v4()))?;
    let mut receipt = Receipt {
        schema: "veoveo.ai/frames-installed-reference/v1",
        synthetic_reference: true,
        retained_append_only: true,
        world_id: Some(world_id.clone()),
        outcome: Some(Outcome::Pending),
        ..Default::default()
    };
    persist(&mut file, &receipt)?;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(120);
    let mut client: Option<SmokeMcpClient> = None;
    let result = tokio::time::timeout_at(deadline, async {
        let token =
            tokio::time::timeout_at(frames_admission_deadline(deadline), installation.token())
                .await
                .map_err(|_| anyhow!("Frames reference OAuth deadline"))?
                .map_err(|_| anyhow!("Frames reference OAuth admission failed"))?;
        client = Some(
            tokio::time::timeout_at(
                frames_admission_deadline(deadline),
                connect_mcp_client(installation.operator.resource.as_str(), &token),
            )
            .await
            .map_err(|_| anyhow!("Frames reference connection deadline"))?
            .map_err(|_| anyhow!("Frames reference connection admission failed"))?,
        );
        let client = client
            .as_ref()
            .context("Frames reference client not admitted")?;
        let create = CreateWorldRequest {
            world_id: world_id.clone(),
            display_name: "Synthetic stream-reference fixture".into(),
            description: Some("Retained synthetic reference; no live producer is claimed".into()),
        };
        receipt.create_intent = Some(create.clone());
        receipt.created = Some(
            tool(
                client,
                &mut file,
                &mut receipt,
                RequestIntent::Create {
                    request: create.clone(),
                },
                "create_world",
                &create,
            )
            .await?,
        );
        persist(&mut file, &receipt)?;
        coverage::require_created(
            &create,
            receipt.created.as_ref().context("missing creation")?,
        )?;
        let publish = PublishWorldRequest {
            world_id: world_id.clone(),
            expected_head_revision_id: None,
            tree: synthetic_tree(&world_id)?,
        };
        receipt.publish_intent = Some(publish.clone());
        receipt.published = Some(
            tool(
                client,
                &mut file,
                &mut receipt,
                RequestIntent::Publish {
                    request: publish.clone(),
                },
                "publish_world",
                &publish,
            )
            .await?,
        );
        persist(&mut file, &receipt)?;
        let revision_uri = receipt
            .published
            .as_ref()
            .context("missing publication")?
            .revision
            .revision_uri()
            .clone();
        receipt.world_read = Some(
            read(
                client,
                &mut file,
                &mut receipt,
                FrameWorldUri::new(&world_id).to_uri()?,
            )
            .await?,
        );
        persist(&mut file, &receipt)?;
        receipt.revision_read =
            Some(read(client, &mut file, &mut receipt, revision_uri.to_uri()?).await?);
        persist(&mut file, &receipt)?;
        receipt.dynamic_node_read = Some(
            read(
                client,
                &mut file,
                &mut receipt,
                WorldFrameUri::new(&revision_uri, &FrameId::parse("dynamic-node")?).to_uri()?,
            )
            .await?,
        );
        persist(&mut file, &receipt)?;
        let descendant = WorldFrameUri::new(&revision_uri, &FrameId::parse("static-descendant")?);
        receipt.descendant_read =
            Some(read(client, &mut file, &mut receipt, descendant.to_uri()?).await?);
        persist(&mut file, &receipt)?;
        require_readback(&receipt)?;
        let request = ConvertFrameRequest {
            target: CoordinateSpace::EcefWgs84,
            points: vec![CoordinatePoint::WorldFrame(WorldFramePosition {
                frame_uri: descendant.clone(),
                x_m: 0.0,
                y_m: 0.0,
                z_m: 0.0,
            })],
            allow_approximation: false,
        };
        receipt.conversion_intent = Some(request.clone());
        let index = admit_request_with(
            &mut receipt,
            RequestIntent::Convert {
                request: request.clone(),
            },
            |receipt| persist(&mut file, receipt),
        )?;
        let name = veoveo_gateway_contract::GatewayToolName::from_parts(
            &veoveo_types::ServerSlug::parse("frames")?,
            &veoveo_types::LocalToolName::parse("convert_frame")?,
        )?;
        receipt.conversion = Some(
            match tokio::time::timeout_at(
                frames_admission_deadline(deadline),
                client.call_tool_once(
                    CallToolRequestParams::new(name.to_string())
                        .with_arguments(serde_json::from_value(serde_json::to_value(&request)?)?),
                ),
            )
            .await
            {
                Ok(Err(error)) => observed_conversion(error),
                Ok(Ok(rmcp::model::CallToolResponse::Complete(response))) => {
                    receipt.requests[index].response = ResponseObservation::Response {
                        digest: digest_response(&response)?,
                    };
                    ConversionObservation::UnexpectedSuccess
                }
                Ok(Ok(_)) => ConversionObservation::UnexpectedSuccess,
                Err(_) => ConversionObservation::Deadline,
            },
        );
        if matches!(
            receipt.requests[index].response,
            ResponseObservation::Awaiting
        ) {
            receipt.requests[index].response = ResponseObservation::ConversionRecorded;
        }
        persist(&mut file, &receipt)?;
        require_conversion_refusal(
            receipt
                .conversion
                .as_ref()
                .context("missing conversion response")?,
        )?;
        let binding = SimulationWorldBinding::from_revision(
            receipt.revision_read.as_ref().context("missing revision")?,
            &descendant,
        );
        receipt.consumer_refused_dynamic =
            Some(matches!(binding, Err(WorldBindingError::DynamicTransform)));
        persist(&mut file, &receipt)?;
        ensure!(
            receipt.consumer_refused_dynamic == Some(true),
            "Current UAV contract consumer did not refuse dynamic ancestry"
        );
        Ok::<(), anyhow::Error>(())
    })
    .await;
    receipt.client_closed = if let Some(client) = client {
        matches!(
            tokio::time::timeout(Duration::from_secs(10), client.cancel()).await,
            Ok(Ok(_))
        )
    } else {
        true
    };
    receipt.outcome = Some(if unresolved(&receipt) {
        Outcome::Unresolved
    } else if matches!(&result, Ok(Ok(()))) && receipt.client_closed {
        Outcome::Passed
    } else {
        Outcome::FailedSettled
    });
    persist(&mut file, &receipt)?;
    result.map_err(|_| anyhow!("Frames reference operation exceeded 120 seconds"))??;
    ensure!(
        receipt.client_closed,
        "Frames reference client cleanup unresolved"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn direct_peer_error_is_required_for_dynamic_refusal() -> Result<()> {
        let owning = rmcp::model::ErrorData::invalid_params(
            "frame `dynamic-node` uses a dynamic transform stream and requires a timestamped recording query",
            None,
        );
        require_conversion_refusal(&observed_conversion(ServiceError::McpError(owning)))?;
        for observed in [
            observed_conversion(ServiceError::McpError(
                rmcp::model::ErrorData::invalid_params("unrelated denial", None),
            )),
            observed_conversion(ServiceError::McpError(
                rmcp::model::ErrorData::internal_error(
                    "frame `dynamic-node` uses a dynamic transform stream and requires a timestamped recording query",
                    None,
                ),
            )),
            observed_conversion(ServiceError::TransportClosed),
            ConversionObservation::UnexpectedSuccess,
            ConversionObservation::Deadline,
        ] {
            assert!(require_conversion_refusal(&observed).is_err());
        }
        Ok(())
    }

    #[test]
    fn current_owner_consumer_rejects_static_descendant_of_dynamic_reference() -> Result<()> {
        let world = FrameWorldId::parse("reference-control")?;
        let tree = synthetic_tree(&world)?;
        let revision = FrameWorldRevision::new(
            FrameWorldRevisionUri::new(&world, &FrameWorldRevisionId::parse("reference-revision")?),
            1.try_into()?,
            ValidatedWorldTree::new(tree)?,
            chrono::Utc::now(),
        );
        let descendant = revision
            .revision_uri()
            .frame(&FrameId::parse("static-descendant")?);
        assert_eq!(
            SimulationWorldBinding::from_revision(&revision, &descendant),
            Err(WorldBindingError::DynamicTransform)
        );
        let geodetic = revision
            .revision_uri()
            .frame(&FrameId::parse("launch-enu")?);
        assert!(SimulationWorldBinding::from_revision(&revision, &geodetic).is_ok());
        let wrong =
            FrameWorldRevisionUri::new(&world, &FrameWorldRevisionId::parse("other-revision")?)
                .frame(&FrameId::parse("static-descendant")?);
        assert_eq!(
            SimulationWorldBinding::from_revision(&revision, &wrong),
            Err(WorldBindingError::DifferentRevision)
        );
        Ok(())
    }

    #[tokio::test]
    async fn settled_creation_survives_cancelled_next_stage_and_private_persistence() -> Result<()>
    {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("reference.json");
        let mut file = admit_frames_evidence(&path)?;
        let world = FrameWorldId::parse("receipt-control")?;
        let request = CreateWorldRequest {
            world_id: world.clone(),
            display_name: "control".into(),
            description: None,
        };
        let output = CreateWorldOutput {
            world: FrameWorldSummary::new(world.clone(), "control".into(), chrono::Utc::now()),
        };
        let mut receipt = Receipt {
            schema: "veoveo.ai/frames-installed-reference/v1",
            create_intent: Some(request),
            created: Some(output.clone()),
            ..Default::default()
        };
        persist(&mut file, &receipt)?;
        let result = tokio::time::timeout(Duration::from_millis(1), async {
            receipt.publish_intent = Some(PublishWorldRequest {
                world_id: world.clone(),
                expected_head_revision_id: None,
                tree: synthetic_tree(&world)?,
            });
            persist(&mut file, &receipt)?;
            std::future::pending::<()>().await;
            Ok::<(), anyhow::Error>(())
        })
        .await;
        assert!(result.is_err());
        assert!(unresolved(&receipt));
        assert_eq!(receipt.created.as_ref(), Some(&output));
        persist(&mut file, &receipt)?;
        let stored: serde_json::Value = serde_json::from_slice(&fs::read(&path)?)?;
        assert_eq!(
            serde_json::from_value::<CreateWorldOutput>(stored["created"].clone())?,
            output
        );
        assert!(!stored["publishIntent"].is_null());
        assert!(stored["published"].is_null());
        assert_eq!(fs::metadata(&path)?.permissions().mode() & 0o777, 0o600);
        assert!(admit_frames_evidence(&path).is_err());
        Ok(())
    }
}

#[cfg(test)]
mod readback_tests {
    use super::*;
    #[test]
    fn installed_reference_readback_preserves_content_and_parent_identity() -> Result<()> {
        let world_id = FrameWorldId::parse("readback-control")?;
        let tree = synthetic_tree(&world_id)?;
        let revision_id = FrameWorldRevisionId::parse("readback-revision")?;
        let at = chrono::Utc::now();
        let revision = FrameWorldRevision::new(
            FrameWorldRevisionUri::new(&world_id, &revision_id),
            1.try_into()?,
            ValidatedWorldTree::new(tree.clone())?,
            at,
        );
        let world = FrameWorldSummary::new(world_id.clone(), "control".into(), at)
            .with_head(revision_id, 1.try_into()?);
        let dynamic = tree
            .frames
            .iter()
            .find(|node| node.frame_id.as_str() == "dynamic-node")
            .context("dynamic")?
            .clone();
        let descendant = tree
            .frames
            .iter()
            .find(|node| node.frame_id.as_str() == "static-descendant")
            .context("descendant")?
            .clone();
        let mut receipt = Receipt {
            created: Some(CreateWorldOutput {
                world: FrameWorldSummary::new(world_id.clone(), "control".into(), at),
            }),
            publish_intent: Some(PublishWorldRequest {
                world_id,
                expected_head_revision_id: None,
                tree,
            }),
            published: Some(PublishWorldOutput {
                world: world.clone(),
                revision: revision.clone(),
                created: true,
            }),
            world_read: Some(world),
            revision_read: Some(revision),
            dynamic_node_read: Some(dynamic.clone()),
            descendant_read: Some(descendant.clone()),
            ..Default::default()
        };
        require_readback(&receipt)?;
        receipt
            .descendant_read
            .as_mut()
            .context("descendant")?
            .parent_frame_id = Some(FrameId::parse("launch-enu")?);
        assert!(require_readback(&receipt).is_err());
        receipt.descendant_read = Some(descendant);
        if let Some(FrameParentTransform::DynamicStream { entity_path, .. }) = receipt
            .dynamic_node_read
            .as_mut()
            .context("dynamic")?
            .parent_transform
            .as_mut()
        {
            *entity_path = FrameEntityPath::new("/changed/body")?;
        }
        assert!(require_readback(&receipt).is_err());
        receipt.dynamic_node_read = Some(dynamic);
        require_readback(&receipt)?;
        let original = receipt
            .published
            .as_ref()
            .context("publication")?
            .world
            .clone();
        for field in 0..3 {
            let mut changed = original.clone();
            match field {
                0 => changed.display_name = "changed display name".into(),
                1 => changed.description = Some("changed description".into()),
                _ => changed.created_at -= chrono::Duration::seconds(1),
            }
            receipt.published.as_mut().context("publication")?.world = changed.clone();
            receipt.world_read = Some(changed);
            assert!(require_readback(&receipt).is_err());
        }
        receipt.published.as_mut().context("publication")?.world = original.clone();
        receipt.world_read = Some(original);
        require_readback(&receipt)?;
        let published = receipt.published.as_mut().context("publication")?;
        published.world = published
            .world
            .clone()
            .with_head(published.revision.revision_id(), 2.try_into()?);
        published.revision = FrameWorldRevision::new(
            published.revision.revision_uri().clone(),
            2.try_into()?,
            ValidatedWorldTree::new(published.revision.tree().clone())?,
            published.revision.created_at(),
        );
        receipt.world_read = Some(published.world.clone());
        receipt.revision_read = Some(published.revision.clone());
        assert!(require_readback(&receipt).is_err());
        Ok(())
    }
}

#[cfg(test)]
mod dispatch_tests {
    use super::*;
    #[tokio::test]
    async fn failed_intent_persistence_does_not_construct_or_poll_rpc() -> Result<()> {
        let constructed = std::cell::Cell::new(0);
        let polled = std::cell::Cell::new(0);
        let request = CreateWorldRequest {
            world_id: FrameWorldId::parse("undispatched-control")?,
            display_name: "control".into(),
            description: None,
        };
        let mut receipt = Receipt {
            create_intent: Some(request.clone()),
            ..Default::default()
        };
        let result = dispatch_after_intent(
            &mut receipt,
            RequestIntent::Create { request },
            |_| bail!("controlled persistence failure"),
            || {
                constructed.set(constructed.get() + 1);
                async {
                    polled.set(polled.get() + 1);
                }
            },
        )
        .await;
        assert!(result.is_err());
        assert_eq!(constructed.get(), 0);
        assert_eq!(polled.get(), 0);
        assert!(receipt.requests.is_empty());
        assert!(receipt.create_intent.is_none());
        assert!(!unresolved(&receipt));
        Ok(())
    }
}
