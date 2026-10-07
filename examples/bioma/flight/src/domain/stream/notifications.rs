//! Public MCP invalidations backed by the installed live GPU session.
use super::*;
use rmcp::{
    ClientLifecycleMode, ClientServiceExt, RoleClient,
    model::{
        ClientCapabilities, ClientConfig, ClientRequest, Implementation, ProtocolVersion,
        ReadResourceRequest, ReadResourceRequestParams, ResourceContents, ServerNotification,
        ServerResult, SubscriptionFilter,
    },
    service::{RunningService, Subscription},
    transport::{
        StreamableHttpClientTransport, streamable_http_client::StreamableHttpClientTransportConfig,
    },
};
use serde::{Serialize, de::DeserializeOwned};
use veoveo_types::ResourceUri;

type Client = RunningService<RoleClient, ClientConfig>;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Report {
    schema: &'static str,
    session: SessionId,
    pipeline: PipelineId,
    cycles: Vec<Cycle>,
    passed: bool,
    failure: Option<String>,
}

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
struct Position {
    processed_frames: u64,
    preview_sequence: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Cycle {
    baseline: Position,
    advanced: Position,
    initial_notifications: usize,
    change_notifications: usize,
    cancelled: bool,
}

pub async fn verify(
    operator: &OperatorClient<'_>,
    session: SessionId,
    acceptance: &StreamScenario,
) -> Result<()> {
    let mut report = Report {
        schema: "veoveo.ai/stream-notification-acceptance/v1",
        session,
        pipeline: acceptance.live_pipeline_id.clone(),
        cycles: Vec::new(),
        passed: false,
        failure: None,
    };
    let result = tokio::time::timeout(Duration::from_secs(60), async {
        let token = operator.installation.token().await?;
        let http = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(10))
            .build()?;
        let transport = StreamableHttpClientTransport::with_client(
            http,
            StreamableHttpClientTransportConfig::with_uri(
                operator.installation.operator.resource.as_str(),
            )
            .auth_header(token),
        );
        let client = ClientConfig::new(
            ClientCapabilities::default(),
            Implementation::new("veoveo-flight-notifications", env!("CARGO_PKG_VERSION")),
        )
        .serve_with_lifecycle(
            transport,
            ClientLifecycleMode::Discover {
                preferred_versions: vec![ProtocolVersion::V_2026_07_28],
            },
        )
        .await?;
        let observed = observe(&client, session, acceptance, &mut report.cycles).await;
        let cleanup = tokio::time::timeout(Duration::from_secs(5), client.cancel()).await;
        observed?;
        cleanup.context("notification client cleanup exceeded five seconds")??;
        Ok::<_, anyhow::Error>(())
    })
    .await
    .context("live Stream notification acceptance exceeded sixty seconds")
    .and_then(|r| r);
    report.passed = result.is_ok();
    report.failure = result.as_ref().err().map(|error| format!("{error:#}"));
    println!("{}", serde_json::to_string(&report)?);
    result
}

async fn observe(
    client: &Client,
    session: SessionId,
    acceptance: &StreamScenario,
    cycles: &mut Vec<Cycle>,
) -> Result<()> {
    let resources = BTreeSet::from([
        stream_uris::session_uri(session).to_uri(),
        stream_uris::session_results_uri(session).to_uri(),
        stream_uris::session_preview_uri(session).to_uri(),
    ]);
    let filter = SubscriptionFilter::builder()
        .resource_subscriptions(resources.iter().map(ToString::to_string))
        .build();
    let mut last: Option<Position> = None;
    for _ in 0..2 {
        let mut subscription = client.listen(filter.clone()).await?;
        ensure!(
            subscription.acknowledged() == &filter,
            "Stream changed the subscription filter"
        );
        let initial_notifications = notifications(&mut subscription, &resources).await?;
        let baseline = position(client, session, acceptance).await?;
        if let Some(previous) = last {
            ensure!(
                baseline.processed_frames >= previous.processed_frames
                    && baseline.preview_sequence >= previous.preview_sequence,
                "reconnected Stream baseline moved backwards"
            );
        }
        let mut change_notifications = 0;
        let advanced = loop {
            change_notifications += notifications(&mut subscription, &resources).await?;
            let current = position(client, session, acceptance).await?;
            ensure!(
                current.processed_frames >= baseline.processed_frames
                    && current.preview_sequence >= baseline.preview_sequence,
                "live Stream state moved backwards"
            );
            if current.processed_frames > baseline.processed_frames
                && current.preview_sequence > baseline.preview_sequence
            {
                break current;
            }
        };
        subscription.cancel().await?;
        ensure!(
            subscription.next().await?.is_none(),
            "cancelled Stream subscription still delivers updates"
        );
        cycles.push(Cycle {
            baseline,
            advanced,
            initial_notifications,
            change_notifications,
            cancelled: true,
        });
        last = Some(advanced);
    }
    Ok(())
}

async fn notifications(
    subscription: &mut Subscription,
    expected: &BTreeSet<ResourceUri>,
) -> Result<usize> {
    let mut seen = BTreeSet::new();
    let mut count = 0;
    while &seen != expected {
        let next = subscription
            .next()
            .await?
            .context("Stream listener closed before its resource updates")?;
        let ServerNotification::ResourceUpdatedNotification(update) = next else {
            bail!("Stream delivered an unrequested notification");
        };
        let uri = ResourceUri::new(update.params.uri)?;
        ensure!(
            expected.contains(&uri),
            "Stream delivered an unrequested resource"
        );
        seen.insert(uri);
        count += 1;
    }
    Ok(count)
}

async fn position(client: &Client, id: SessionId, acceptance: &StreamScenario) -> Result<Position> {
    let session: LiveSessionView = read(client, &stream_uris::session_uri(id).to_uri()).await?;
    let results: LiveResultsView =
        read(client, &stream_uris::session_results_uri(id).to_uri()).await?;
    let preview: LivePreviewView =
        read(client, &stream_uris::session_preview_uri(id).to_uri()).await?;
    ensure!(
        session.session_id() == id
            && results.session_id == id
            && preview.session_id == id
            && session.pipeline_id() == &acceptance.live_pipeline_id
            && results.pipeline_id == acceptance.live_pipeline_id,
        "live Stream notification read returned another session or pipeline"
    );
    ensure!(
        session.lifecycle == LiveSessionLifecycle::Running,
        "live Stream session is {:?}: {}",
        session.lifecycle,
        session.error.as_deref().unwrap_or("no diagnostic")
    );
    ensure!(
        results.processed_frames >= acceptance.minimum_live_frames,
        "live Stream lost its inference baseline"
    );
    let observed = results
        .frames
        .last()
        .context("live Stream has no inference frame")?;
    let observed_at = DateTime::parse_from_rfc3339(&observed.observed_at)?.with_timezone(&Utc);
    let age = Utc::now()
        .signed_duration_since(observed_at)
        .num_milliseconds();
    ensure!(
        age >= 0 && age <= i64::try_from(acceptance.maximum_result_age_ms)?,
        "live Stream inference is stale"
    );
    validate_live_preview(&preview.chunks)?;
    let chunk = preview
        .chunks
        .last()
        .context("live Stream has no encoded preview")?;
    Ok(Position {
        processed_frames: results.processed_frames,
        preview_sequence: chunk.sequence,
    })
}

async fn read<T: DeserializeOwned>(client: &Client, uri: &ResourceUri) -> Result<T> {
    // Each invalidation must be followed by an actual source read, not SDK cache reuse.
    let request = ClientRequest::ReadResourceRequest(ReadResourceRequest::new(
        ReadResourceRequestParams::new(uri.as_str()),
    ));
    let ServerResult::ReadResourceResult(result) = client.send_request(request).await? else {
        bail!("unexpected live Stream resource response");
    };
    let [
        ResourceContents::TextResourceContents {
            uri: returned,
            text,
            ..
        },
    ] = result.contents.as_slice()
    else {
        bail!("live Stream resource must contain one JSON body");
    };
    ensure!(
        returned == uri.as_str(),
        "live Stream resource returned a different URI"
    );
    serde_json::from_str(text).with_context(|| format!("decoding live Stream resource {uri}"))
}
