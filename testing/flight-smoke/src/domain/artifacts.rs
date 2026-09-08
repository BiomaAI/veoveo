use super::*;

// Verify governed bytes through the public gateway. Importing an Artifact or
// Recording implementation here would defeat the external-client build boundary.
pub(super) async fn assert_governed_artifact_access(
    conformance: &Path,
    base: &str,
    artifact_id: &str,
) -> Result<()> {
    let admin_token = gateway_token_for_context(
        conformance,
        base,
        "admin-service",
        "admin",
        &["operator:use", "admin:manage"],
        "operations",
    )
    .await?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(60))
        .build()?;
    let snapshot: Value = client
        .get(format!("{base}/admin/admin/console/snapshot"))
        .bearer_auth(&admin_token)
        .send()
        .await
        .context("requesting the governed Console snapshot")?
        .error_for_status()
        .context("governed Console snapshot returned an error")?
        .json()
        .await
        .context("decoding the governed Console snapshot")?;
    let artifact = snapshot
        .get("artifacts")
        .and_then(Value::as_array)
        .and_then(|artifacts| {
            artifacts
                .iter()
                .find(|artifact| artifact.get("id").and_then(Value::as_str) == Some(artifact_id))
        })
        .with_context(|| format!("Console snapshot omitted governed artifact {artifact_id}"))?;
    ensure!(
        artifact
            .pointer("/provenance/workContext")
            .and_then(Value::as_str)
            == Some("operations")
            && artifact
                .pointer("/provenance/producer")
                .and_then(Value::as_str)
                .is_some_and(|producer| producer.ends_with("#operator-service"))
            && artifact
                .pointer("/provenance/invocationMode")
                .and_then(Value::as_str)
                == Some("automated")
            && artifact
                .pointer("/provenance/policyRevision")
                .and_then(Value::as_str)
                .is_some_and(|revision| !revision.is_empty())
            && artifact
                .pointer("/outputOwner/kind")
                .and_then(Value::as_str)
                == Some("group")
            && artifact.pointer("/outputOwner/id").and_then(Value::as_str) == Some("operations")
            && artifact
                .pointer("/effectiveAccess/read")
                .and_then(Value::as_bool)
                == Some(true),
        "governed artifact provenance or effective access is incomplete: {artifact}"
    );

    let download_url = format!("{base}/artifacts/operator/{artifact_id}/download");
    let preview_json = download_governed_json_artifact(conformance, base, artifact_id).await?;
    ensure!(
        preview_json.is_object(),
        "authorized governed artifact preview did not contain a JSON object"
    );

    let independent_token = gateway_token_for_context(
        conformance,
        base,
        "operator-service",
        "operator",
        OPERATOR_PROFILE_SCOPES,
        "independent-review",
    )
    .await?;
    let no_redirect = reqwest::Client::builder()
        .timeout(Duration::from_secs(60))
        .redirect(reqwest::redirect::Policy::none())
        .build()?;
    let denied = no_redirect
        .get(download_url)
        .bearer_auth(independent_token)
        .send()
        .await
        .context("requesting the governed artifact from an independent Work Context")?;
    ensure!(
        denied.status() == reqwest::StatusCode::FORBIDDEN,
        "independent Work Context received {}, expected 403",
        denied.status()
    );
    Ok(())
}

pub(super) async fn download_governed_json_artifact(
    conformance: &Path,
    base: &str,
    artifact_id: &str,
) -> Result<Value> {
    let token = gateway_token(conformance, base).await?;
    let response = reqwest::Client::builder()
        .timeout(Duration::from_secs(60))
        .build()?
        .get(format!("{base}/artifacts/operator/{artifact_id}/download"))
        .bearer_auth(token)
        .send()
        .await
        .with_context(|| format!("downloading governed JSON artifact {artifact_id}"))?
        .error_for_status()
        .with_context(|| format!("governed JSON artifact {artifact_id} returned an error"))?;
    let media_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_owned();
    ensure!(
        media_type == "application/json"
            || (media_type.starts_with("application/") && media_type.ends_with("+json")),
        "governed artifact {artifact_id} returned media type `{media_type}`"
    );
    serde_json::from_slice(&response.bytes().await?)
        .with_context(|| format!("governed artifact {artifact_id} contained invalid JSON"))
}

pub(super) async fn wait_for_recording_camera_range(
    operator: &OperatorClient<'_>,
    dataset_id: &str,
    recording_id: &str,
    camera_entity: &str,
    range_start: i64,
    range_end: i64,
    timeout: Duration,
) -> Result<Value> {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        let projection = operator
            .call_tool(
                "recording__create_recording_projection",
                serde_json::json!({
                    "dataset_id": dataset_id,
                    "recording_id": recording_id,
                    "entity_paths": [camera_entity],
                    "component_ids": ["VideoStream:sample"],
                    "timeline": "simulation_time",
                    "sampling": {
                        "kind": "range",
                        "start": range_start,
                        "end": range_end
                    },
                    "sparse_fill": "none",
                    "maximum_entities": 1,
                    "maximum_columns": 1,
                    "maximum_samples": 1000,
                    "maximum_rows": 10000,
                    "maximum_bytes": 33554432,
                    "deadline_ms": 15000,
                    "idempotency_key": uuid::Uuid::now_v7().to_string(),
                    "units": {},
                    "coordinate_frame_refs": []
                }),
            )
            .await?;
        if projection
            .pointer("/result/row_count")
            .and_then(Value::as_u64)
            .is_some_and(|count| count > 0)
        {
            return Ok(projection);
        }
        if tokio::time::Instant::now() >= deadline {
            bail!(
                "Recording Catalog exposed no durable live UAV camera samples in range \
                 {range_start}..={range_end} within {timeout:?}: {projection}"
            );
        }
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}

pub(super) fn assert_live_recording_snapshot(output: &Value, domain: &str) -> Result<()> {
    let sources = output
        .pointer("/source_snapshot/sources")
        .and_then(Value::as_array)
        .with_context(|| format!("{domain} omitted its governed recording source snapshot"))?;
    ensure!(
        sources.iter().any(|source| {
            source.get("kind").and_then(Value::as_str) == Some("live_ingest_part")
        }),
        "{domain} did not analyze an acknowledged live ingest part before archive rollover: \
         {output}"
    );
    Ok(())
}

pub(super) fn assert_requested_range(
    output: &Value,
    start: i64,
    end: i64,
    domain: &str,
) -> Result<()> {
    ensure!(
        output
            .pointer("/summary/requested_start_index")
            .and_then(Value::as_i64)
            == Some(start)
            && output
                .pointer("/summary/requested_end_index")
                .and_then(Value::as_i64)
                == Some(end),
        "{domain} did not analyze the requested near-live recording range: {output}"
    );
    Ok(())
}
