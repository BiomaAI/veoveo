use super::*;
use axum::{body::Body, http::header};
fn line(value: &AuditExportLine) -> Result<Vec<u8>, std::io::Error> {
    let mut bytes = serde_json::to_vec(value).map_err(std::io::Error::other)?;
    bytes.push(b'\n');
    Ok(bytes)
}

pub(crate) async fn export_audit(
    State(state): State<AdminState>,
    Path(profile): Path<GatewayProfileId>,
    Extension(subject): Extension<AuthenticatedSubject>,
    Query(parameters): Query<Parameters<AuditQuery>>,
) -> Response {
    let mut query = parameters.query.0;
    if query.validate().is_err() {
        return StatusCode::BAD_REQUEST.into_response();
    }
    // An export always includes the full filtered range, independent of the open page.
    query.cursor = None;
    let scope = match admit(&state, &profile, &subject, &query.partition).await {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let Some(slot) = state.console_stream.acquire(subject.principal.id.as_str()) else {
        return StatusCode::TOO_MANY_REQUESTS.into_response();
    };
    let store = state.control_store.platform_store().clone();
    let marker = match subject.audit_draft(
        &profile,
        AuditTarget::AuditLog {
            partition: query.partition.clone(),
        },
        AuditDetail::Read {
            method: AuditReadMethod::AuditExport,
        },
        AuditOutcome::Allowed,
        AuditReason::Accepted,
    ) {
        Ok(draft) => draft,
        Err(_) => return StatusCode::SERVICE_UNAVAILABLE.into_response(),
    };
    let marker_id = marker.id();
    let marker_partition = marker.partition().clone();
    if state.gateway_state.record_audit(marker).await.is_err() {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    // Sealing a marker proves the sealer crossed every earlier committed record,
    // including records whose UUID was allocated before a later-committing write.
    if !matches!(
        tokio::time::timeout(
            READ_DEADLINE,
            store.audit_wait_sealed(&scope, &marker_partition, marker_id)
        )
        .await,
        Ok(Ok(()))
    ) {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            "Audit sealing has not reached this export; retry when the sealer is healthy.",
        )
            .into_response();
    }
    let range = match tokio::time::timeout(
        READ_DEADLINE,
        store.audit_export_range(&scope, &query.partition),
    )
    .await
    {
        Ok(Ok(range)) => range,
        _ => return StatusCode::SERVICE_UNAVAILABLE.into_response(),
    };
    let tail = range.checkpoint;
    let deadline = tokio::time::Instant::now()
        + (subject.access_token.expires_at - Utc::now())
            .to_std()
            .unwrap_or_default()
            .min(Duration::from_secs(600));
    let catalog = state.catalog.subscribe();
    let output: futures::stream::BoxStream<'static, Result<Vec<u8>, std::io::Error>> = Box::pin(
        async_stream::try_stream! {
            let _slot = slot;
            yield line(&AuditExportLine::Header { query: Box::new(query.clone()), first: range.first, checkpoint: tail.clone() })?;
            let mut after = None;
            let mut count = 0u64;
            if let (Some(tail), Some(first)) = (&tail, range.first) {
                let mut expected = first.get();
                while after != Some(tail.sequence) {
                    if catalog.has_changed().unwrap_or(true) || tokio::time::timeout_at(deadline, current(&state, &profile, &subject)).await != Ok(true) {
                        Err(std::io::Error::other("audit export authority or deadline expired"))?;
                    }
                    let blocks = tokio::time::timeout_at(deadline.min(tokio::time::Instant::now() + READ_DEADLINE), store.audit_blocks(&scope, &query.partition, after, 16)).await
                        .map_err(std::io::Error::other)?.map_err(std::io::Error::other)?;
                    if blocks.is_empty() { Err(std::io::Error::other("audit export lost its sealed range"))?; }
                    for block in blocks {
                        if block.head.sequence > tail.sequence || block.head.sequence.get() != expected {
                            Err(std::io::Error::other("audit export range changed during retention"))?;
                        }
                        expected += 1;
                        let records = tokio::time::timeout_at(deadline.min(tokio::time::Instant::now() + READ_DEADLINE), store.audit_filtered_block_records(&scope, &block, &query)).await
                            .map_err(std::io::Error::other)?.map_err(std::io::Error::other)?;
                        for record in records {
                            if tokio::time::Instant::now() >= deadline || catalog.has_changed().unwrap_or(true) {
                                Err(std::io::Error::other("audit export authority or deadline expired"))?;
                            }
                            count = count.checked_add(1).ok_or_else(|| std::io::Error::other("audit export count exhausted"))?;
                            yield line(&AuditExportLine::Record { record: Box::new(record) })?;
                        }
                        after = Some(block.head.sequence);
                        if after == Some(tail.sequence) { break; }
                    }
                }
            }
            yield line(&AuditExportLine::Complete { records: count, checkpoint: tail.clone() })?;
        },
    );
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/x-ndjson")
        .header(
            header::CONTENT_DISPOSITION,
            "attachment; filename=veoveo-audit.jsonl",
        )
        .header(header::CACHE_CONTROL, "no-store")
        .body(Body::from_stream(output))
        .expect("static export response headers")
}
