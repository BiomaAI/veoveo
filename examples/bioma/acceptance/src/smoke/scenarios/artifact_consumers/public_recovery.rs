//! Normal-OAuth readback of the original public upload after the shared restart.
use super::*;
use veoveo_artifact_mcp::contract::{ArtifactIndexCursor, ArtifactResource};
use veoveo_types::ResourceUri;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Proof {
    metadata: veoveo_artifact_contract::ArtifactMetadata,
    catalog_pages: usize,
    byte_len: usize,
    sha256: UploadSha256,
}

// Read-only wire views of Artifact MCP's exported index, whose production type
// is Serialize-only. Owner URI admission still supplies its identity semantics.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct IndexPage {
    items: Vec<IndexEntry>,
    next_cursor: Option<ArtifactIndexCursor>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct IndexEntry {
    uri: ResourceUri,
    title: String,
    mime_type: String,
}

pub(super) async fn verify(
    installation: &InstalledTarget,
    public: &PublicUploadContext<'_>,
    peer: &rmcp::Peer<rmcp::RoleClient>,
    receipt: &ArtifactUploadReceipt,
) -> Result<Proof> {
    let mut response = public
        .client
        .get(format!(
            "{}/artifacts/{}/{}/download",
            installation.public_base(),
            installation.profile(),
            receipt.artifact_id,
        ))
        .bearer_auth(public.token)
        .send()
        .await?
        .error_for_status()?;
    let mut bytes = Vec::with_capacity(FOCUSED_BYTES.len());
    while let Some(chunk) = response.chunk().await? {
        ensure!(
            bytes.len() + chunk.len() <= FOCUSED_BYTES.len(),
            "recovered public download exceeded fixture bound"
        );
        bytes.extend_from_slice(&chunk);
    }
    let sha256 = UploadSha256::parse(hex::encode(Sha256::digest(&bytes)))?;
    ensure!(
        bytes == FOCUSED_BYTES && sha256 == receipt.sha256,
        "recovered public bytes or digest changed"
    );
    let metadata_uri = ArtifactResource::Metadata(receipt.artifact_id).to_uri();
    let metadata: veoveo_artifact_contract::ArtifactMetadata = serde_json::from_value(
        veoveo_testing_support::read_mcp_resource_json(peer, metadata_uri.as_str()).await?,
    )?;
    ensure!(
        metadata.artifact_id() == receipt.artifact_id
            && metadata.artifact_uri == receipt.artifact_uri
            && metadata.byte_len == receipt.byte_len
            && metadata.filename.as_deref() == Some(receipt.filename.as_str())
            && metadata.mime_type.as_deref() == Some(receipt.mime_type.as_str())
            && metadata.compliance.work_context.as_ref()
                == Some(&installation.operator.work_context.id),
        "recovered public metadata changed occurrence, descriptor or context"
    );
    let mut cursor = None;
    let mut cursors = BTreeSet::new();
    let mut members = BTreeSet::new();
    for page_number in 1..=256 {
        let uri = ArtifactResource::Index { cursor }.to_uri();
        let page: IndexPage = serde_json::from_value(
            veoveo_testing_support::read_mcp_resource_json(peer, uri.as_str()).await?,
        )?;
        ensure!(
            page.items.len() <= 100 && (!page.items.is_empty() || page.next_cursor.is_none()),
            "recovered catalog page violates owner bounds"
        );
        let mut found = false;
        for entry in page.items {
            let ArtifactResource::Metadata(id) = ArtifactResource::parse(entry.uri.as_str())?
            else {
                anyhow::bail!("recovered catalog entry is not an Artifact metadata identity");
            };
            ensure!(
                members.insert(id) && entry.mime_type == "application/json",
                "recovered catalog repeats or changes a member"
            );
            if id == receipt.artifact_id {
                ensure!(
                    entry.uri == metadata_uri && entry.title == receipt.filename,
                    "recovered catalog changed selected identity"
                );
                found = true;
            }
        }
        if found {
            return Ok(Proof {
                metadata,
                catalog_pages: page_number,
                byte_len: bytes.len(),
                sha256,
            });
        }
        let next = page
            .next_cursor
            .context("recovered catalog omitted original upload occurrence")?;
        ensure!(
            cursors.insert(next.after()),
            "recovered catalog repeated cursor"
        );
        cursor = Some(next);
    }
    anyhow::bail!("recovered catalog exceeded 256 pages")
}
