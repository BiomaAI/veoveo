//! Observed resource replies are journaled before payload admission.
use super::*;
use base64::engine::general_purpose::STANDARD;
use evidence::{ReadObservation, ReadOutcome};
use rmcp::model::{ReadResourceRequestParams, ReadResourceResult, ResourceContents};
use sha2::{Digest, Sha256};
use veoveo_mcp_conformance::client::failure::ObservedFailure;
use veoveo_timeseries_mcp::contract::{TimeseriesUsageIndexUri, TimeseriesUsagePage};
use veoveo_types::{ResourceUri, Sha256Digest, TaskId};

pub(super) enum Reply {
    Received(ReadResourceResult),
    Denied(ObservedFailure),
}
pub(super) async fn fetch(
    client: &SmokeMcpClient,
    target: &ResourceUri,
    file: &mut std::fs::File,
    receipt: &mut Receipt,
) -> Result<Reply> {
    ensure!(
        receipt.reads.len() < 80,
        "Timeseries resource request budget exhausted"
    );
    let index = receipt.reads.len();
    receipt.reads.push(ReadObservation {
        target: target.clone(),
        outcome: ReadOutcome::Pending,
    });
    persist(file, receipt)?;
    match tokio::time::timeout(
        Duration::from_secs(15),
        client.read_resource(ReadResourceRequestParams::new(target.as_str())),
    )
    .await
    {
        Ok(Ok(response)) => {
            let bytes = serde_json::to_vec(&response)?;
            receipt.reads[index].outcome = ReadOutcome::Received {
                digest: Sha256Digest::from_bytes(Sha256::digest(&bytes).into()),
            };
            persist(file, receipt)?;
            ensure!(
                bytes.len() <= 3 * 1024 * 1024,
                "Timeseries resource response exceeds three MiB"
            );
            Ok(Reply::Received(response))
        }
        Ok(Err(rmcp::ServiceError::McpError(error))) => {
            let failure = ObservedFailure::mcp(i64::from(error.code.0), error.message.as_ref());
            receipt.reads[index].outcome = ReadOutcome::Mcp {
                failure: failure.clone(),
            };
            persist(file, receipt)?;
            Ok(Reply::Denied(failure))
        }
        Ok(Err(_)) => {
            receipt.reads[index].outcome = ReadOutcome::Transport;
            persist(file, receipt)?;
            bail!("Timeseries resource transport failed; see private receipt")
        }
        Err(_) => {
            receipt.reads[index].outcome = ReadOutcome::Timeout;
            persist(file, receipt)?;
            bail!("Timeseries resource deadline; see private receipt")
        }
    }
}
fn received(reply: Reply) -> Result<ReadResourceResult> {
    match reply {
        Reply::Received(response) => Ok(response),
        Reply::Denied(_) => bail!("Timeseries resource was denied; see private receipt"),
    }
}
pub(super) async fn json<T: serde::de::DeserializeOwned>(
    client: &SmokeMcpClient,
    target: &ResourceUri,
    file: &mut std::fs::File,
    receipt: &mut Receipt,
) -> Result<T> {
    let response = received(fetch(client, target, file, receipt).await?)?;
    let [ResourceContents::TextResourceContents { uri, text, .. }] = response.contents.as_slice()
    else {
        bail!("Timeseries resource requires one JSON body")
    };
    ensure!(
        uri == target.as_str() && text.len() <= 65536,
        "Timeseries JSON resource identity or size differs"
    );
    serde_json::from_str(text)
        .map_err(|_| anyhow!("Timeseries JSON resource failed owner admission"))
}
pub(super) async fn blob(
    client: &SmokeMcpClient,
    target: &ResourceUri,
    file: &mut std::fs::File,
    receipt: &mut Receipt,
) -> Result<Vec<u8>> {
    let response = received(fetch(client, target, file, receipt).await?)?;
    let [
        ResourceContents::BlobResourceContents {
            uri,
            blob,
            mime_type,
            ..
        },
    ] = response.contents.as_slice()
    else {
        bail!("Timeseries Artifact requires one blob")
    };
    ensure!(
        uri == target.as_str() && mime_type.as_deref() == Some(assertions::RRD_MIME),
        "Timeseries Artifact URI or MIME differs"
    );
    ensure!(
        blob.len() <= assertions::MAX_RRD.div_ceil(3) * 4,
        "Timeseries encoded Artifact exceeds byte budget"
    );
    let bytes = STANDARD
        .decode(blob)
        .map_err(|_| anyhow!("Timeseries Artifact blob is not base64"))?;
    ensure!(
        bytes.len() <= assertions::MAX_RRD,
        "Timeseries Artifact exceeds two MiB"
    );
    Ok(bytes)
}
pub(super) async fn usage_ids(
    client: &SmokeMcpClient,
    file: &mut std::fs::File,
    receipt: &mut Receipt,
) -> Result<BTreeSet<TaskId>> {
    let mut cursor = None;
    let mut ids = BTreeSet::new();
    let mut previous = None;
    for _ in 0..32 {
        let target = TimeseriesUsageIndexUri::new(cursor.as_ref()).to_uri()?;
        let page: TimeseriesUsagePage = json(client, &target, file, receipt).await?;
        for entry in page.usage() {
            let id = entry.task_id();
            ensure!(
                previous.is_none_or(|prior| prior < id) && ids.insert(id),
                "Timeseries usage pages repeat or reorder Task identities"
            );
            previous = Some(id);
        }
        if let Some(next) = page.next_cursor() {
            cursor = Some(next.clone());
        } else {
            return Ok(ids);
        }
    }
    bail!("Timeseries usage exceeds32page observation budget")
}
