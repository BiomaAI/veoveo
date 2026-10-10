#[path = "artifact_consumers/capability_recovery.rs"]
mod capability_recovery;
#[path = "artifact_consumers/public_recovery.rs"]
mod public_recovery;
#[path = "artifact_consumers/public_upload.rs"]
mod public_upload;
#[path = "artifact_consumers/python.rs"]
mod python;
use super::*;
use anyhow::ensure;
use base64::engine::general_purpose::STANDARD;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::os::unix::fs::OpenOptionsExt;
use std::{collections::BTreeSet, num::NonZeroU32};
use tokio::io::AsyncWriteExt;
use veoveo_artifact_contract::ArtifactUploadReceipt as BrowserReceipt;
use veoveo_artifact_contract::{
    ArtifactUploadError, ArtifactUploadReceipt, ArtifactUploadSession, CompleteArtifactUpload,
    CreateArtifactUpload, EffectiveArtifactUploadPolicy, UPLOAD_PART_BYTE_LEN_HEADER,
    UPLOAD_PART_SHA256_HEADER, UploadErrorCode, UploadPartReceipt, UploadSha256,
};
use veoveo_bioma_acceptance::reports::artifact_upload::{
    BrowserUploadReport as BrowserEvidence, BrowserUploadReportSchema,
};
use veoveo_mcp_contract::*;

#[derive(Debug)]
pub(crate) enum ArtifactConsumerProfile {
    Browser { evidence: std::path::PathBuf },
    Focused,
}
impl ArtifactConsumerProfile {
    pub(crate) fn admit(focused: bool, browser: Option<std::path::PathBuf>) -> Result<Self> {
        match (focused, browser) {
            (false, Some(evidence)) => Ok(Self::Browser { evidence }),
            (true, None) => Ok(Self::Focused),
            _ => anyhow::bail!(
                "browser profile requires browser evidence; focused profile forbids it"
            ),
        }
    }
}
const FOCUSED_BYTES: [u8; 2048] = [b'a'; 2048];
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FocusedEvidence {
    schema: &'static str,
    source_revision: String,
    public_base_url: String,
    bounded_receipt: ArtifactUploadReceipt,
    python: python::Observation,
    parquet_receipt: ArtifactUploadReceipt,
    unknown_length_receipt: ArtifactUploadReceipt,
    checks: Vec<&'static str>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Evidence {
    schema: &'static str,
    source_revision: String,
    public_base_url: String,
    large_receipt: BrowserReceipt,
    python: python::Observation,
    parquet_receipt: ArtifactUploadReceipt,
    unknown_length_receipt: ArtifactUploadReceipt,
    checks: Vec<&'static str>,
}

pub(crate) async fn artifact_upload_consumers(
    conformance: &Path,
    installation: &InstalledTarget,
    selected: ArtifactConsumerProfile,
    evidence_output: &Path,
    service_recovery: bool,
) -> Result<()> {
    ensure!(
        !evidence_output.exists(),
        "consumer evidence already exists"
    );
    // Admit opt-in fixture selection before the existing upload side effects.
    if service_recovery {
        capability_recovery::admit(installation, evidence_output)?;
    }
    let focused = matches!(selected, ArtifactConsumerProfile::Focused);
    let browser_receipts = match selected {
        ArtifactConsumerProfile::Browser { evidence } => {
            let browser: BrowserEvidence = serde_json::from_slice(&fs::read(evidence)?)?;
            ensure!(
                browser.schema == BrowserUploadReportSchema::Upload,
                "expected current browser upload report"
            );
            browser.check_receipts()?;
            let large = browser
                .large_receipt
                .context("large upload receipt missing")?;
            ensure!(
                large.byte_len > u64::from(u32::MAX),
                "large fixture must exceed 4 GiB"
            );
            Some((
                large,
                browser.csv_receipt.context("CSV upload receipt missing")?,
            ))
        }
        ArtifactConsumerProfile::Focused => None,
    };
    let journal = public_upload::Journal::create(evidence_output, focused)?;
    let base = installation.public_base();
    let profile = installation.profile();
    let comparison_context = installation
        .target
        .operator
        .comparison_context
        .as_deref()
        .context("artifact-upload-consumers requires operator.comparisonContext")?;
    ensure!(
        installation.target.artifact_consumer.is_some(),
        "artifact-upload-consumers requires artifactConsumer in the installation target"
    );
    let token = installation.token().await?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(60))
        .redirect(Policy::none())
        .build()?;
    let upload_base = format!("{base}/artifacts/{profile}/uploads");
    let policy: EffectiveArtifactUploadPolicy = client
        .get(format!("{base}/artifacts/{profile}/upload-policy"))
        .bearer_auth(&token)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    ensure!(
        policy.allowed,
        "installed profile did not authorize machine uploads"
    );

    if let Some((large, _)) = &browser_receipts {
        let other_actor = client
            .get(format!("{upload_base}/{}", large.upload_id))
            .bearer_auth(&token)
            .send()
            .await?;
        ensure!(
            other_actor.status() == StatusCode::NOT_FOUND,
            "foreign-actor upload status returned {}, expected concealed 404",
            other_actor.status()
        );
        ensure!(
            other_actor.json::<ArtifactUploadError>().await?.code == UploadErrorCode::NotFound,
            "foreign upload response did not come from the typed upload boundary"
        );
    }
    // A real Parquet producer supplies bytes; protocol assertions remain here in Rust.
    let temporary = tempfile::tempdir()?;
    let parquet = temporary.path().join("upload-consumer.parquet");
    let db = ::duckdb::Connection::open_in_memory()?;
    db.execute_batch(&format!("COPY (SELECT * FROM (VALUES ('alpha', 1), ('beta', 2)) AS t(name, value)) TO '{}' (FORMAT PARQUET)", parquet.display().to_string().replace('\'', "''")))?;
    let parquet_bytes = fs::read(&parquet)?;
    let parquet_receipt = upload_small(
        &client,
        &token,
        &upload_base,
        SmallUploadFixture {
            filename: "upload-consumer.parquet",
            mime: "application/vnd.apache.parquet",
            bytes: parquet_bytes,
            known: true,
        },
        &journal,
    )
    .await?;
    let unknown_length_receipt = upload_small(
        &client,
        &token,
        &upload_base,
        SmallUploadFixture {
            filename: "upload-consumer.csv",
            mime: "text/csv",
            bytes: b"name,value\nalpha,1\nbeta,2\n".to_vec(),
            known: false,
        },
        &journal,
    )
    .await?;
    let (large_receipt, csv_receipt) = match browser_receipts {
        Some(receipts) => receipts,
        None => {
            let bounded = upload_small(
                &client,
                &token,
                &upload_base,
                SmallUploadFixture {
                    filename: "focused-artifact.bin",
                    mime: "application/octet-stream",
                    bytes: FOCUSED_BYTES.to_vec(),
                    known: true,
                },
                &journal,
            )
            .await?;
            (bounded, unknown_length_receipt.clone())
        }
    };
    let independent = installation.token_for_context(comparison_context).await?;
    let denied = client
        .get(format!(
            "{base}/artifacts/{profile}/{}/download",
            large_receipt.artifact_id
        ))
        .bearer_auth(independent)
        .send()
        .await?;
    ensure!(
        denied.status() == StatusCode::FORBIDDEN,
        "independent context received {}",
        denied.status()
    );

    if focused {
        let mut response = client
            .get(format!(
                "{base}/artifacts/{profile}/{}/download",
                large_receipt.artifact_id
            ))
            .bearer_auth(&token)
            .send()
            .await?
            .error_for_status()?;
        let mut bytes = Vec::with_capacity(FOCUSED_BYTES.len());
        while let Some(chunk) = response.chunk().await? {
            ensure!(
                bytes.len() + chunk.len() <= FOCUSED_BYTES.len(),
                "public focused download exceeded fixture bytes"
            );
            bytes.extend_from_slice(&chunk);
        }
        ensure!(
            bytes == FOCUSED_BYTES
                && large_receipt.sha256.as_str() == hex::encode(Sha256::digest(FOCUSED_BYTES)),
            "public focused bytes or digest changed"
        );
    }
    for uri in [
        &csv_receipt.artifact_uri,
        &parquet_receipt.artifact_uri,
        &unknown_length_receipt.artifact_uri,
    ] {
        let arguments = serde_json::to_string(&serde_json::json!({"datasetUri":uri,"rows":2}))?;
        let result = super::run_public_conformance(
            conformance,
            base,
            profile,
            &token,
            &[
                "call",
                "--tool-name",
                "datasheet__preview_dataset",
                "--arguments",
                &arguments,
            ],
            Duration::from_secs(60),
        )
        .await?;
        let result = super::structured_output(&result)?;
        ensure!(
            result.get("rowCount").and_then(Value::as_u64) == Some(2),
            "Datasheet did not consume both rows: {result}"
        );
        let rows = result
            .get("rows")
            .and_then(Value::as_array)
            .context("missing preview rows")?;
        ensure!(
            rows.len() == 2
                && rows[0].get("name").and_then(Value::as_str) == Some("alpha")
                && rows[1].get("value").and_then(Value::as_i64) == Some(2),
            "Datasheet changed uploaded values"
        );
    }
    println!(
        "Public known/unknown-length upload, immutable part retry, context denial, and Python CSV/Parquet MCP consumption passed"
    );

    let python = python::consume(installation, &large_receipt, &csv_receipt).await?;
    ensure!(
        python.pages.len() >= 2 && python.pages.len() <= 256,
        "Python SDK did not traverse multiple Artifact pages within the fixture bound"
    );
    let mut seen = BTreeSet::new();
    let mut previous = None;
    for (index, page) in python.pages.iter().enumerate() {
        ensure!(
            page.artifacts.len() <= 1,
            "Artifact page exceeded requested limit"
        );
        for metadata in &page.artifacts {
            let id = metadata.artifact_id();
            ensure!(seen.insert(id), "Artifact paging repeated an occurrence");
            ensure!(
                previous.is_none_or(|prior| id < prior),
                "Artifact paging did not advance its keyset"
            );
            previous = Some(id);
        }
        if index + 1 == python.pages.len() {
            ensure!(
                page.next_cursor.is_none(),
                "Artifact traversal did not reach its final page"
            );
        } else {
            ensure!(
                page.artifacts.len() == 1 && page.next_cursor == previous,
                "Artifact continuation does not identify the last returned occurrence"
            );
        }
    }
    for receipt in [&large_receipt, &csv_receipt] {
        let metadata = python
            .pages
            .iter()
            .flat_map(|page| &page.artifacts)
            .find(|metadata| metadata.artifact_id() == receipt.artifact_id)
            .context("Artifact pages omitted a selected upload occurrence")?;
        ensure!(
            metadata.artifact_uri == receipt.artifact_uri
                && metadata.filename.as_deref() == Some(receipt.filename.as_str())
                && metadata.mime_type.as_deref() == Some(receipt.mime_type.as_str())
                && metadata.byte_len == receipt.byte_len
                && metadata.compliance.work_context.as_ref()
                    == Some(&installation.operator.work_context.id),
            "Artifact pages changed a selected occurrence or Work Context"
        );
    }
    ensure!(
        python.foreign_page.artifacts.is_empty() && python.foreign_page.next_cursor.is_none(),
        "Artifact paging exposed members to the isolated synthetic tenant"
    );
    for metadata in [&python.metadata, &python.resolved_metadata] {
        ensure!(
            metadata.artifact_id() == csv_receipt.artifact_id
                && metadata.artifact_uri == csv_receipt.artifact_uri
                && metadata.byte_len == csv_receipt.byte_len
                && metadata.mime_type.as_deref() == Some(csv_receipt.mime_type.as_str())
                && metadata.filename.as_deref() == Some(csv_receipt.filename.as_str())
                && metadata.compliance.work_context.as_ref()
                    == Some(&installation.operator.work_context.id),
            "Python metadata/address consumer changed the selected CSV occurrence"
        );
    }
    ensure!(
        python.resolved_sha256 == csv_receipt.sha256.as_str(),
        "Python URI resolution changed the selected CSV bytes"
    );
    ensure!(
        python.bytes == large_receipt.byte_len && python.sha256 == large_receipt.sha256.as_str(),
        "Python large-file byte or hash mismatch"
    );
    ensure!(
        python.peak_rss_kib < 256 * 1024 && python.max_chunk_bytes <= 1024 * 1024,
        "Python selected download exceeded its bounded working set"
    );
    ensure!(
        python.early_bytes > 0
            && python.early_bytes <= 1024 * 1024
            && python.small_limit_error == "ArtifactTooLarge",
        "Python early exit or byte ceiling failed"
    );
    ensure!(
        python.materialized_inside
            && !python.materialized_after
            && python.csv_sha256 == csv_receipt.sha256.as_str(),
        "Python materialization or cleanup failed"
    );
    ensure!(
        matches!(
            python.foreign_tenant_error.as_str(),
            "ArtifactDenied" | "ArtifactNotFound"
        ),
        "Python foreign tenant could consume the artifact"
    );
    if service_recovery {
        capability_recovery::run(
            installation,
            evidence_output,
            PublicUploadContext {
                client: &client,
                token: &token,
                base: &upload_base,
                journal: &journal,
            },
        )
        .await?;
    }
    let revision = run_checked(Path::new("git"), ["rev-parse".into(), "HEAD".into()], [])?
        .trim()
        .to_owned();
    let evidence = if focused {
        serde_json::to_vec_pretty(&FocusedEvidence {
            schema: "veoveo.ai/artifact-focused-consumer-acceptance/v1",
            source_revision: revision,
            public_base_url: base.into(),
            bounded_receipt: large_receipt,
            python,
            parquet_receipt,
            unknown_length_receipt,
            checks: vec![
                "Normal OAuth upload admission, immutable part retry, changed part refusal and completion replay",
                "Independent public bounded bytes and digest; direct SDK metadata, URI resolution, paging and tenant isolation",
                "CSV and Parquet consumed through normal OAuth Datasheet MCP",
            ],
        })?
    } else {
        serde_json::to_vec_pretty(&Evidence {
            schema: "veoveo.ai/artifact-upload-consumer-acceptance/v3",
            source_revision: revision,
            public_base_url: base.into(),
            large_receipt,
            python,
            parquet_receipt,
            unknown_length_receipt,
            checks: vec![
                "Public machine OAuth and actor/Work Context denial",
                "Known and unknown length uploads with immutable retry and completion replay",
                "Real CSV and Parquet consumed through public Datasheet MCP",
                "Installed Python SDK streamed the entire large object with exact SHA-256",
                "Direct-plane Python SDK metadata and URI resolution preserve the selected CSV occurrence",
                "Direct-plane Python SDK limit-one catalog traversal preserves both selected occurrences and tenant isolation",
                "Bounded Python memory, early exit, byte ceiling, temporary-file cleanup, and tenant denial",
            ],
        })?
    };
    journal.passed()?;
    if let Some(parent) = evidence_output.parent() {
        fs::create_dir_all(parent)?;
    }
    use std::io::Write;
    let mut output = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(evidence_output)?;
    output.write_all(&evidence)?;
    output.sync_all()?;
    println!(
        "Installed Artifact consumers passed. Evidence: {}",
        evidence_output.display()
    );
    Ok(())
}

struct SmallUploadFixture<'a> {
    filename: &'a str,
    mime: &'a str,
    bytes: Vec<u8>,
    known: bool,
}

async fn upload_small(
    client: &reqwest::Client,
    token: &str,
    base: &str,
    fixture: SmallUploadFixture<'_>,
    journal: &public_upload::Journal,
) -> Result<ArtifactUploadReceipt> {
    upload_small_across_replacement(
        PublicUploadContext {
            client,
            token,
            base,
            journal,
        },
        fixture,
        None::<std::future::Ready<Result<()>>>,
    )
    .await
}

struct PublicUploadContext<'a> {
    client: &'a reqwest::Client,
    token: &'a str,
    base: &'a str,
    journal: &'a public_upload::Journal,
}

async fn upload_small_across_replacement(
    context: PublicUploadContext<'_>,
    fixture: SmallUploadFixture<'_>,
    replacement: Option<impl std::future::Future<Output = Result<()>>>,
) -> Result<ArtifactUploadReceipt> {
    let PublicUploadContext {
        client,
        token,
        base,
        journal,
    } = context;
    let SmallUploadFixture {
        filename,
        mime,
        bytes,
        known,
    } = fixture;
    let sha = UploadSha256::parse(hex::encode(Sha256::digest(&bytes)))?;
    let descriptor = CreateArtifactUpload {
        filename: filename.into(),
        mime_type: mime.into(),
        byte_len: known.then_some(bytes.len() as u64),
        sha256: Some(sha.clone()),
    };
    let key = uuid::Uuid::now_v7().to_string();
    let owned = journal.begin(client, token, base, &key, &descriptor)?;
    let response = client
        .post(base)
        .bearer_auth(token)
        .header("Idempotency-Key", &key)
        .json(&descriptor)
        .send()
        .await?;
    ensure!(
        response.status() == StatusCode::CREATED,
        "public admission returned {}",
        response.status()
    );
    let session: ArtifactUploadSession = response.json().await?;
    owned.acknowledge(session.upload_id)?;
    let url = format!("{base}/{}", session.upload_id);
    let result = async {
        ensure!(
            bytes.len() as u64 <= session.layout.part_bytes.get(),
            "small fixture requires multiple parts"
        );
        let part_url = format!("{url}/parts/1");
        let send_part = || {
            client
                .put(&part_url)
                .bearer_auth(token)
                .header(UPLOAD_PART_BYTE_LEN_HEADER, bytes.len())
                .header(UPLOAD_PART_SHA256_HEADER, sha.as_str())
                .body(bytes.clone())
        };
        owned.intent(session.upload_id, "partAndExactReplay")?;
        let first: UploadPartReceipt = send_part().send().await?.error_for_status()?.json().await?;
        owned.accepted(&session, &first)?;
        ensure!(
            first.part_number.get() == 1
                && first.sha256 == sha
                && first.byte_len == bytes.len() as u64,
            "accepted part changed bytes or identity"
        );
        if let Some(replacement) = replacement {
            // No completion has been requested. Persist the accepted part and
            // authoritative Open session before permission for the one restart.
            let unfinished: ArtifactUploadSession = client
                .get(&url)
                .bearer_auth(token)
                .send()
                .await?
                .error_for_status()?
                .json()
                .await?;
            owned.unfinished(&unfinished, &first)?;
            admit_unfinished(&session, &unfinished, &first)?;
            owned.intent(session.upload_id, "serviceReplacement")?;
            replacement.await?;
            let resumed: ArtifactUploadSession = client
                .get(&url)
                .bearer_auth(token)
                .send()
                .await?
                .error_for_status()?
                .json()
                .await?;
            owned.resumed(&resumed)?;
            admit_unfinished(&unfinished, &resumed, &first)?;
            ensure!(
                resumed.expires_at == unfinished.expires_at,
                "replacement changed unfinished upload expiry"
            );
        }
        let repeat: UploadPartReceipt =
            send_part().send().await?.error_for_status()?.json().await?;
        ensure!(
            first == repeat && first.sha256 == sha && first.byte_len == bytes.len() as u64,
            "accepted part replay differs"
        );
        let mut changed = bytes.clone();
        changed[0] ^= 1;
        owned.intent(session.upload_id, "changedPartRefusal")?;
        let conflict = client
            .put(&part_url)
            .bearer_auth(token)
            .header(UPLOAD_PART_BYTE_LEN_HEADER, changed.len())
            .header(
                UPLOAD_PART_SHA256_HEADER,
                hex::encode(Sha256::digest(&changed)),
            )
            .body(changed)
            .send()
            .await?;
        ensure!(
            conflict.status() == StatusCode::CONFLICT,
            "changed accepted part returned {}",
            conflict.status()
        );
        let manifest = CompleteArtifactUpload {
            byte_len: bytes.len() as u64,
            part_count: NonZeroU32::new(1).unwrap(),
            sha256: Some(sha.clone()),
        };
        owned.intent(session.upload_id, "complete")?;
        let response = client
            .post(format!("{url}/complete"))
            .bearer_auth(token)
            .json(&manifest)
            .send()
            .await?
            .error_for_status()?;
        let mut status: ArtifactUploadSession = response.json().await?;
        let deadline = tokio::time::Instant::now() + Duration::from_secs(90);
        while status.receipt.is_none() {
            ensure!(
                !status.state.is_terminal() && tokio::time::Instant::now() < deadline,
                "upload did not publish a receipt: {:?}",
                status.state
            );
            tokio::time::sleep(Duration::from_millis(500)).await;
            status = client
                .get(&url)
                .bearer_auth(token)
                .send()
                .await?
                .error_for_status()?
                .json()
                .await?;
        }
        let receipt = status
            .receipt
            .clone()
            .context("completion receipt missing")?;
        owned.received(&receipt)?;
        ensure!(
            status.upload_id == session.upload_id
                && status.state == veoveo_artifact_contract::ArtifactUploadState::Completed,
            "completion changed upload identity or state"
        );
        ensure!(
            receipt.upload_id == session.upload_id
                && receipt.sha256 == sha
                && receipt.byte_len == bytes.len() as u64
                && receipt.filename == filename
                && receipt.mime_type == mime,
            "uploaded object changed bytes"
        );
        owned.published(&receipt)?;
        owned.intent(session.upload_id, "completionReplay")?;
        let replay: ArtifactUploadSession = client
            .post(format!("{url}/complete"))
            .bearer_auth(token)
            .json(&manifest)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        ensure!(
            replay.receipt.as_ref() == Some(&receipt),
            "completion replay changed occurrence"
        );
        owned.intent(session.upload_id, "completedCancellationRefusal")?;
        ensure!(
            client
                .delete(&url)
                .bearer_auth(token)
                .send()
                .await?
                .status()
                == StatusCode::CONFLICT,
            "completed upload was cancelled"
        );
        Ok::<_, anyhow::Error>(receipt)
    }
    .await;
    if result.is_err() {
        owned
            .cancel()
            .await
            .context("public upload failed; owned cleanup also failed")?;
    }
    result
}

fn admit_unfinished(
    original: &ArtifactUploadSession,
    current: &ArtifactUploadSession,
    part: &UploadPartReceipt,
) -> Result<()> {
    ensure!(
        current.upload_id == original.upload_id
            && current.created_at == original.created_at
            && current.descriptor == original.descriptor
            && current.layout == original.layout
            && current.state == veoveo_artifact_contract::ArtifactUploadState::Open
            && current.receipt.is_none()
            && current.failure.is_none()
            && current.accepted_bytes == part.byte_len
            && current.accepted_part_count == 1
            && current.parts.as_slice() == std::slice::from_ref(part)
            && current.next_part_cursor.is_none(),
        "unfinished upload identity, accepted part or state changed"
    );
    Ok(())
}
