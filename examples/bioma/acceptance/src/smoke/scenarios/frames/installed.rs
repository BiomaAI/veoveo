use super::*;
#[path = "coverage.rs"]
mod coverage;

/// Installed qualification uses the same owner scenario and never replays a mutation.
pub(crate) async fn frames_installed(
    installation: &support::InstalledTarget,
    evidence: &Path,
) -> Result<()> {
    use rmcp::model::SubscriptionFilter;
    use veoveo_frames_mcp::contract::*;
    use veoveo_testing_support::connect_mcp_client;

    let deadline = tokio::time::Instant::now() + Duration::from_secs(300);
    let mut evidence_file = admit_frames_evidence(evidence)?;
    let world_id = FrameWorldId::parse(format!("acceptance-{}", uuid::Uuid::new_v4()))?;
    let world_uri = FrameWorldUri::new(&world_id);
    let worlds_uri = FrameWorldsUri::new(None);
    let mut receipt = InstalledFramesReceipt {
        schema_version: "veoveo.ai/frames-installed-evidence/v2",
        world_id: world_id.clone(),
        revision_uri: None,
        outcome: InstalledFramesOutcome::FailedBeforeMutation,
        cleanup: InstalledFramesCleanup::NotOpened,
        retained_append_only: true,
        mutations: Vec::new(),
        worlds_pages: 0,
        usage_pages: 0,
        task_subscription_cleanup: false,
        first_error: None,
        usage: Vec::new(),
    };
    write_frames_evidence(&mut evidence_file, &receipt)?;
    let token = tokio::time::timeout_at(frames_admission_deadline(deadline), installation.token())
        .await
        .map_err(|_| anyhow!("Frames OAuth admission deadline exceeded"))?
        .map_err(|_| anyhow!("Frames OAuth admission failed before mutation"))?;
    let client = tokio::time::timeout_at(
        frames_admission_deadline(deadline),
        connect_mcp_client(installation.operator.resource.as_str(), &token),
    )
    .await
    .map_err(|_| anyhow!("Frames MCP connection admission deadline exceeded"))?
    .map_err(|_| anyhow!("Frames MCP connection admission failed before mutation"))?;
    receipt.cleanup = InstalledFramesCleanup::Unresolved;
    let filter = SubscriptionFilter::builder()
        .resource_subscriptions([worlds_uri.to_string()])
        .build();
    let mut subscription = match tokio::time::timeout_at(
        frames_admission_deadline(deadline),
        client.listen(filter.clone()),
    )
    .await
    {
        Ok(Ok(subscription)) => subscription,
        _ => {
            if matches!(
                tokio::time::timeout(Duration::from_secs(5), client.cancel()).await,
                Ok(Ok(_))
            ) {
                receipt.cleanup = InstalledFramesCleanup::ConnectionsClosed;
            }
            let _ = write_frames_evidence(&mut evidence_file, &receipt);
            bail!("installed Frames worlds subscription failed before mutation");
        }
    };
    let mut worlds_closed = false;
    let mut immutable_subscription = None;
    let mut owned_listeners = Vec::new();
    let mut foreign_client = None;
    let result = tokio::time::timeout_at(deadline, async {
        anyhow::ensure!(
            subscription.acknowledged() == &filter,
            "Frames subscription changed its filter"
        );
        // Listen sends a current resource invalidation baseline. Consume it before dispatch
        // so a baseline cannot masquerade as the authored change.
        let initial = tokio::time::timeout(Duration::from_secs(15), subscription.next())
            .await
            .context("Frames baseline wait exceeded fifteen seconds")?
            .map_err(|_| anyhow!("Frames baseline stream observation unresolved"))?
            .context("Frames subscription ended before baseline")?;
        require_worlds_invalidation(initial, worlds_uri.as_str())?;
        let foreign = installation.administrator()?;
        anyhow::ensure!(
            foreign.principal != installation.operator.principal,
            "Frames visibility requires a distinct administrator principal"
        );
        let foreign_token =
            tokio::time::timeout_at(frames_admission_deadline(deadline), foreign.token())
                .await
                .map_err(|_| anyhow!("Frames foreign OAuth admission deadline"))?
                .map_err(|_| anyhow!("Frames foreign OAuth admission failed"))?;
        foreign_client = Some(
            tokio::time::timeout_at(
                frames_admission_deadline(deadline),
                connect_mcp_client(foreign.resource.as_str(), &foreign_token),
            )
            .await
            .map_err(|_| anyhow!("Frames foreign MCP admission deadline"))?
            .map_err(|_| anyhow!("Frames foreign MCP admission failed"))?,
        );
        coverage::admit_catalogs(&client, &world_id).await?;
        let created: CreateWorldOutput = coverage::create(
            &client,
            &mut evidence_file,
            &mut receipt,
            CreateWorldRequest {
                world_id: world_id.clone(),
                display_name: "Installed acceptance world".into(),
                description: Some("Retained deterministic Frames acceptance fixture".into()),
            },
        )
        .await?;
        anyhow::ensure!(
            created.world.world_id() == world_id,
            "Frames created a different world"
        );
        let update = tokio::time::timeout(Duration::from_secs(15), subscription.next())
            .await
            .context("Frames authored invalidation wait exceeded fifteen seconds")?
            .map_err(|_| anyhow!("Frames authored invalidation stream observation unresolved"))?
            .context("Frames subscription ended before authored invalidation")?;
        require_worlds_invalidation(update, worlds_uri.as_str())?;
        worlds_closed = matches!(
            tokio::time::timeout(Duration::from_secs(5), subscription.cancel()).await,
            Ok(Ok(_))
        );
        let owned: FrameWorldSummary = installed_frames_read(&client, world_uri.as_str()).await?;
        anyhow::ensure!(
            owned == created.world,
            "Frames read returned a different world"
        );
        let published = coverage::identical_publication(
            &client,
            &mut evidence_file,
            &mut receipt,
            PublishWorldRequest {
                world_id: world_id.clone(),
                expected_head_revision_id: None,
                tree: installed_frames_tree()?,
            },
            &created.world,
        )
        .await?;
        anyhow::ensure!(
            published.created
                && published.world.world_id() == world_id
                && published.revision.world_id() == world_id,
            "Frames publication did not create the owned revision"
        );
        let revision_uri = published.revision.revision_uri().clone();
        receipt.revision_uri = Some(revision_uri.clone());
        write_frames_evidence(&mut evidence_file, &receipt)?;
        let revision: FrameWorldRevision =
            installed_frames_read(&client, revision_uri.as_str()).await?;
        anyhow::ensure!(
            revision == published.revision && revision.tree() == &installed_frames_tree()?,
            "Frames immutable revision differs from the published deterministic tree"
        );
        let immutable_filter = SubscriptionFilter::builder()
            .resource_subscriptions([revision_uri.to_string()])
            .build();
        immutable_subscription::require_rejection(
            client.peer(),
            immutable_filter,
            frames_admission_deadline(deadline),
            &mut immutable_subscription,
        )
        .await?;
        coverage::run(
            &client,
            foreign_client
                .as_ref()
                .expect("foreign connection admitted"),
            &mut evidence_file,
            &mut receipt,
            &published,
            &mut owned_listeners,
            deadline,
        )
        .await?;
        receipt.outcome = InstalledFramesOutcome::Passed;
        Ok::<(), anyhow::Error>(())
    })
    .await;
    let mut additional_closed = true;
    for listener in &mut owned_listeners {
        additional_closed &= listener.close().await;
    }
    receipt.task_subscription_cleanup = additional_closed;
    let foreign_closed = if let Some(client) = foreign_client {
        matches!(
            tokio::time::timeout(Duration::from_secs(5), client.cancel()).await,
            Ok(Ok(_))
        )
    } else {
        true
    };
    let immutable_closed = immutable_subscription::cancel_owned(&mut immutable_subscription).await;
    let listener_closed = worlds_closed
        || matches!(
            tokio::time::timeout(Duration::from_secs(5), subscription.cancel()).await,
            Ok(Ok(_))
        );
    let client_closed = matches!(
        tokio::time::timeout(Duration::from_secs(5), client.cancel()).await,
        Ok(Ok(_))
    );
    if immutable_closed && listener_closed && client_closed && additional_closed && foreign_closed {
        receipt.cleanup = InstalledFramesCleanup::ConnectionsClosed;
    }
    let written = write_frames_evidence(&mut evidence_file, &receipt);
    if let Some(error) = receipt.first_error.take() {
        return Err(error);
    }
    // Preserve the first domain/observation failure even if receipt persistence also fails.
    result.context("installed Frames acceptance exceeded 300 seconds")??;
    written?;
    anyhow::ensure!(
        immutable_closed && listener_closed && client_closed && additional_closed && foreign_closed,
        "Frames connection cleanup remains unresolved"
    );
    Ok(())
}

fn frames_admission_deadline(overall: tokio::time::Instant) -> tokio::time::Instant {
    overall.min(tokio::time::Instant::now() + Duration::from_secs(15))
}
fn admit_frames_evidence(path: &Path) -> Result<File> {
    use std::os::unix::fs::OpenOptionsExt;
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .map_err(|_| anyhow!("Frames evidence requires a new writable private file"))
}
fn write_frames_evidence(file: &mut File, receipt: &InstalledFramesReceipt) -> Result<()> {
    use std::io::{Seek, SeekFrom, Write};
    let bytes = serde_json::to_vec_pretty(receipt)?;
    file.seek(SeekFrom::Start(0))?;
    file.write_all(&bytes)?;
    file.set_len(bytes.len().try_into()?)?;
    file.flush()?;
    file.sync_all()?;
    Ok(())
}
fn record_frames_mutation_intent(
    file: &mut File,
    receipt: &mut InstalledFramesReceipt,
) -> Result<()> {
    receipt.outcome = InstalledFramesOutcome::MutationUnresolved;
    write_frames_evidence(file, receipt)
}
fn require_complete_frames_response(
    response: rmcp::model::CallToolResponse,
) -> Result<rmcp::model::CallToolResult> {
    match response {
        rmcp::model::CallToolResponse::Complete(result) => Ok(result),
        _ => bail!("Frames mutation requires one complete response; outcome remains unresolved"),
    }
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct InstalledFramesReceipt {
    schema_version: &'static str,
    world_id: veoveo_frames_mcp::contract::FrameWorldId,
    revision_uri: Option<veoveo_frames_mcp::contract::FrameWorldRevisionUri>,
    outcome: InstalledFramesOutcome,
    cleanup: InstalledFramesCleanup,
    retained_append_only: bool,
    mutations: Vec<coverage::Mutation>,
    worlds_pages: usize,
    usage_pages: usize,
    task_subscription_cleanup: bool,
    #[serde(skip)]
    first_error: Option<anyhow::Error>,
    usage: Vec<veoveo_mcp_contract::UsageReport>,
}
#[derive(serde::Serialize)]
#[serde(rename_all = "snake_case")]
enum InstalledFramesOutcome {
    FailedBeforeMutation,
    MutationUnresolved,
    Passed,
}
#[derive(serde::Serialize)]
#[serde(rename_all = "snake_case")]
enum InstalledFramesCleanup {
    NotOpened,
    Unresolved,
    ConnectionsClosed,
}

fn require_worlds_invalidation(
    notification: rmcp::model::ServerNotification,
    expected: &str,
) -> Result<()> {
    match notification {
        rmcp::model::ServerNotification::ResourceUpdatedNotification(update)
            if update.params.uri == expected =>
        {
            Ok(())
        }
        _ => bail!("Frames delivered an unexpected resource notification"),
    }
}
async fn installed_frames_call<T: serde::de::DeserializeOwned>(
    client: &veoveo_testing_support::SmokeMcpClient,
    name: &str,
    request: impl serde::Serialize,
) -> Result<T> {
    let local = veoveo_types::LocalToolName::parse(name)?;
    let name = veoveo_gateway_contract::GatewayToolName::from_parts(
        &veoveo_mcp_contract::ServerSlug::parse("frames")?,
        &local,
    )?;
    let response = tokio::time::timeout(
        Duration::from_secs(15),
        client.call_tool_once(
            rmcp::model::CallToolRequestParams::new(name.to_string())
                .with_arguments(serde_json::from_value(serde_json::to_value(request)?)?),
        ),
    )
    .await
    .map_err(|_| anyhow!("Frames mutation response deadline; outcome unresolved"))?
    .map_err(|_| anyhow!("Frames mutation response unresolved"))?;
    let response = require_complete_frames_response(response)?;
    anyhow::ensure!(
        response.is_error != Some(true),
        "Frames tool returned an error"
    );
    serde_json::from_value(
        response
            .structured_content
            .context("Frames response omitted structured content")?,
    )
    .map_err(|_| anyhow!("Frames mutation response failed owner admission"))
}
async fn installed_frames_read<T: serde::de::DeserializeOwned>(
    client: &veoveo_testing_support::SmokeMcpClient,
    uri: &str,
) -> Result<T> {
    use rmcp::model::{ReadResourceRequestParams, ResourceContents};
    let result = tokio::time::timeout(
        Duration::from_secs(15),
        client.read_resource(ReadResourceRequestParams::new(uri)),
    )
    .await
    .map_err(|_| anyhow!("Frames resource read deadline exceeded"))?
    .map_err(|_| anyhow!("installed Frames resource read failed"))?;
    let [
        ResourceContents::TextResourceContents {
            uri: returned,
            text,
            ..
        },
    ] = result.contents.as_slice()
    else {
        bail!("Frames resource must return one JSON body");
    };
    anyhow::ensure!(returned == uri, "Frames resource returned a different URI");
    serde_json::from_str(text).map_err(|_| anyhow!("Frames resource failed owner admission"))
}

#[cfg(test)]
mod installed_frames_tests {
    use super::*;
    #[test]
    fn evidence_admission_refuses_occupied_symlink_and_unwritable_destinations() -> Result<()> {
        use std::io::Write;
        use std::os::unix::fs::{PermissionsExt, symlink};
        let directory = tempfile::tempdir()?;
        let occupied = directory.path().join("occupied.json");
        fs::write(&occupied, b"retained-private-sentinel")?;
        assert!(admit_frames_evidence(&occupied).is_err());
        let link = directory.path().join("symlink.json");
        symlink(&occupied, &link)?;
        assert!(admit_frames_evidence(&link).is_err());
        assert!(admit_frames_evidence(&occupied.join("unwritable.json")).is_err());
        assert!(
            admit_frames_evidence(Path::new("/proc/self/frames-acceptance-receipt.json")).is_err()
        );
        assert_eq!(fs::read(&occupied)?, b"retained-private-sentinel");
        let path = directory.path().join("receipt.json");
        let mut handle = admit_frames_evidence(&path)?;
        assert_eq!(handle.metadata()?.permissions().mode() & 0o777, 0o600);
        let reserved = directory.path().join("reserved.json");
        fs::rename(&path, &reserved)?;
        symlink(&occupied, &path)?;
        handle.write_all(b"owned-receipt")?;
        handle.flush()?;
        assert_eq!(fs::read(reserved)?, b"owned-receipt");
        assert_eq!(fs::read(occupied)?, b"retained-private-sentinel");
        Ok(())
    }
    #[test]
    fn one_dispatch_refuses_input_required_without_using_request_state() -> Result<()> {
        use rmcp::model::{CallToolResponse, CallToolResult, InputRequiredResult};
        let input = CallToolResponse::InputRequired(InputRequiredResult::from_request_state(
            "private-never-redispatched-state",
        ));
        let error = require_complete_frames_response(input).unwrap_err();
        assert!(
            !error
                .to_string()
                .contains("private-never-redispatched-state")
        );
        require_complete_frames_response(CallToolResponse::Complete(CallToolResult::default()))?;
        Ok(())
    }
    #[tokio::test]
    async fn admission_budget_is_capped_by_overall_deadline() {
        let expired = tokio::time::Instant::now() - Duration::from_secs(1);
        assert_eq!(frames_admission_deadline(expired), expired);
        assert!(
            tokio::time::timeout_at(
                frames_admission_deadline(expired),
                std::future::pending::<()>()
            )
            .await
            .is_err()
        );
        let overall = tokio::time::Instant::now() + Duration::from_secs(1);
        assert_eq!(frames_admission_deadline(overall), overall);
    }
    #[test]
    fn persisted_intent_precedes_dispatch_and_retains_observed_revision() -> Result<()> {
        use veoveo_frames_mcp::contract::{FrameWorldId, FrameWorldRevisionId, FrameWorldUri};
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("receipt.json");
        let mut file = admit_frames_evidence(&path)?;
        let world = FrameWorldId::parse("acceptance-intent")?;
        let mut receipt = InstalledFramesReceipt {
            schema_version: "veoveo.ai/frames-installed-evidence/v2",
            world_id: world.clone(),
            revision_uri: None,
            outcome: InstalledFramesOutcome::FailedBeforeMutation,
            cleanup: InstalledFramesCleanup::NotOpened,
            retained_append_only: true,
            mutations: Vec::new(),
            worlds_pages: 0,
            usage_pages: 0,
            task_subscription_cleanup: false,
            first_error: None,
            usage: Vec::new(),
        };
        write_frames_evidence(&mut file, &receipt)?;
        record_frames_mutation_intent(&mut file, &mut receipt)?;
        let persisted: Value = serde_json::from_slice(&fs::read(&path)?)?;
        assert_eq!(persisted["outcome"], "mutation_unresolved");
        let revision = FrameWorldUri::new(&world).revision(&FrameWorldRevisionId::parse("rev-1")?);
        receipt.revision_uri = Some(revision.clone());
        write_frames_evidence(&mut file, &receipt)?;
        let persisted: Value = serde_json::from_slice(&fs::read(&path)?)?;
        assert_eq!(persisted["revisionUri"], revision.to_string());
        assert_eq!(persisted["outcome"], "mutation_unresolved");
        let mut read_only = File::open(&path)?;
        let dispatched = record_frames_mutation_intent(&mut read_only, &mut receipt).is_ok();
        assert!(
            !dispatched,
            "failed intent persistence must refuse dispatch"
        );
        assert_eq!(fs::read(&path)?, serde_json::to_vec_pretty(&receipt)?);
        Ok(())
    }
    #[test]
    fn worlds_notification_requires_exact_owned_collection() -> Result<()> {
        use rmcp::model::{
            ResourceUpdatedNotification, ResourceUpdatedNotificationParam, ServerNotification,
        };
        let update = |uri: &str| {
            ServerNotification::ResourceUpdatedNotification(ResourceUpdatedNotification::new(
                ResourceUpdatedNotificationParam::new(uri),
            ))
        };
        require_worlds_invalidation(update("frames://worlds"), "frames://worlds")?;
        for uri in [
            "frames://world/other",
            "frames://worlds?cursor=foreign",
            "view://views",
        ] {
            assert!(require_worlds_invalidation(update(uri), "frames://worlds").is_err());
        }
        Ok(())
    }
    #[test]
    fn installed_fixture_tree_and_receipt_preserve_owner_context() -> Result<()> {
        use veoveo_frames_mcp::contract::{FrameWorldId, FrameWorldUri};
        let world = FrameWorldId::parse("acceptance-fixture")?;
        let revision = FrameWorldUri::new(&world).revision(
            &veoveo_frames_mcp::contract::FrameWorldRevisionId::parse("rev-1")?,
        );
        assert_eq!(revision.world_id(), world);
        assert_eq!(installed_frames_tree()?.frames.len(), 3);
        let receipt = InstalledFramesReceipt {
            schema_version: "veoveo.ai/frames-installed-evidence/v2",
            world_id: world,
            revision_uri: Some(revision),
            outcome: InstalledFramesOutcome::MutationUnresolved,
            cleanup: InstalledFramesCleanup::ConnectionsClosed,
            retained_append_only: true,
            mutations: Vec::new(),
            worlds_pages: 0,
            usage_pages: 0,
            task_subscription_cleanup: false,
            first_error: None,
            usage: Vec::new(),
        };
        let value = serde_json::to_value(receipt)?;
        assert_eq!(value["outcome"], "mutation_unresolved");
        assert_eq!(value["cleanup"], "connections_closed");
        assert_eq!(value["retainedAppendOnly"], true);
        assert!(value.get("world_id").is_none());
        Ok(())
    }
}
